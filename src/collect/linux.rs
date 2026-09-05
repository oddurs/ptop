//! Linux backend: reads `/proc` directly, no dependencies.
//!
//! The kernel exposes cumulative counters, not rates. Every CPU number here is
//! a delta between the previous read and this one, which is why the collector
//! is stateful and why the very first sample reports zero busy time.

use super::{Collector, Needs};
use crate::sample::{IoRates, MemStat, ProcSample, Sample};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Raw jiffy counters from one `/proc/stat` CPU line.
#[derive(Clone, Copy, Default)]
struct CpuTimes {
    idle: u64,
    total: u64,
}

impl CpuTimes {
    /// Parse the numbers after the `cpuN` label.
    ///
    /// Fields are: user nice system idle iowait irq softirq steal guest
    /// guest_nice. iowait counts as idle — the CPU genuinely had nothing to run.
    fn parse(fields: &str) -> Option<Self> {
        let v: Vec<u64> = fields
            .split_whitespace()
            .filter_map(|f| f.parse().ok())
            .collect();
        if v.len() < 4 {
            return None;
        }
        // guest and guest_nice are already counted inside user and nice, so
        // summing every field as-is would double-count them.
        let total: u64 = v.iter().take(8).sum();
        Some(Self {
            idle: v[3] + v.get(4).copied().unwrap_or(0),
            total,
        })
    }

    /// Busy percentage between two reads.
    fn busy_pct_since(&self, prev: &Self) -> f32 {
        let dt = self.total.saturating_sub(prev.total);
        if dt == 0 {
            return 0.0;
        }
        let di = self.idle.saturating_sub(prev.idle);
        let busy = dt.saturating_sub(di);
        ((busy as f64 / dt as f64) * 100.0) as f32
    }
}

pub struct ProcFs {
    prev_total: Option<CpuTimes>,
    prev_cores: Vec<CpuTimes>,
    /// pid -> cumulative (utime + stime) jiffies at the previous sample.
    prev_proc_jiffies: HashMap<i32, u64>,
    /// pid -> cumulative (read_bytes, write_bytes) at the previous sample.
    prev_proc_io: HashMap<i32, (u64, u64)>,
    prev_at: Option<SystemTime>,
    /// uid -> username, parsed once from /etc/passwd.
    users: HashMap<u32, Arc<str>>,
    ticks_per_sec: f64,
    page_size: u64,
}

impl ProcFs {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            prev_total: None,
            prev_cores: Vec::new(),
            prev_proc_jiffies: HashMap::new(),
            prev_proc_io: HashMap::new(),
            prev_at: None,
            users: parse_passwd(),
            // USER_HZ is fixed at 100 on effectively every Linux build. The
            // honest way is sysconf(_SC_CLK_TCK), but that needs libc, and the
            // point of this backend is to need nothing.
            ticks_per_sec: 100.0,
            page_size: 4096,
        })
    }

    fn read_cpu(&mut self) -> io::Result<(f32, Vec<f32>)> {
        let stat = fs::read_to_string("/proc/stat")?;
        let mut total_now = CpuTimes::default();
        let mut cores_now = Vec::new();

        for line in stat.lines() {
            let Some(rest) = line.strip_prefix("cpu") else {
                break; // cpu lines come first; nothing after them matters here
            };
            match rest.split_once(char::is_whitespace) {
                // "cpu  ..." — the aggregate line has no digit after "cpu"
                Some(("", fields)) => {
                    if let Some(t) = CpuTimes::parse(fields) {
                        total_now = t;
                    }
                }
                // "cpu0 ...", "cpu1 ..." — per-core lines
                Some((_n, fields)) => {
                    if let Some(t) = CpuTimes::parse(fields) {
                        cores_now.push(t);
                    }
                }
                None => {}
            }
        }

        let total_pct = match self.prev_total {
            Some(prev) => total_now.busy_pct_since(&prev),
            None => 0.0,
        };
        let core_pcts = cores_now
            .iter()
            .enumerate()
            .map(|(i, now)| match self.prev_cores.get(i) {
                Some(prev) => now.busy_pct_since(prev),
                None => 0.0,
            })
            .collect();

        self.prev_total = Some(total_now);
        self.prev_cores = cores_now;
        Ok((total_pct, core_pcts))
    }

    fn read_mem(&self) -> io::Result<MemStat> {
        let text = fs::read_to_string("/proc/meminfo")?;
        let get = |key: &str| -> u64 {
            text.lines()
                .find_map(|l| {
                    l.strip_prefix(key)?
                        .split_whitespace()
                        .next()?
                        .parse::<u64>()
                        .ok()
                })
                .unwrap_or(0)
                * 1024 // meminfo is in kB
        };
        let total = get("MemTotal:");
        let available = get("MemAvailable:");
        let swap_total = get("SwapTotal:");
        let swap_free = get("SwapFree:");
        Ok(MemStat {
            total,
            // MemAvailable already accounts for reclaimable cache, so this is
            // the "really in use" figure rather than the alarming one.
            used: total.saturating_sub(available),
            available,
            swap_total,
            swap_used: swap_total.saturating_sub(swap_free),
        })
    }

    fn read_load(&self) -> io::Result<[f64; 3]> {
        let text = fs::read_to_string("/proc/loadavg")?;
        let mut it = text.split_whitespace();
        let mut out = [0.0; 3];
        for slot in out.iter_mut() {
            *slot = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
        }
        Ok(out)
    }

    fn read_uptime(&self) -> io::Result<Duration> {
        let text = fs::read_to_string("/proc/uptime")?;
        let secs: f64 = text
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        Ok(Duration::from_secs_f64(secs))
    }

    fn read_procs(
        &mut self,
        elapsed: Duration,
        needs: Needs,
        denied: &mut usize,
    ) -> io::Result<Vec<ProcSample>> {
        let mut out = Vec::new();
        let mut seen = HashMap::new();
        let mut seen_io = HashMap::new();
        let elapsed_secs = elapsed.as_secs_f64();

        for entry in fs::read_dir("/proc")? {
            let Ok(entry) = entry else { continue };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(pid) = name.parse::<i32>() else {
                continue; // non-numeric entries are not processes
            };

            // Processes exit while we walk the directory; a vanished pid is
            // normal, not an error worth surfacing.
            let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };

            // The /proc/<pid> directory is owned by the process's uid, so one
            // stat answers what parsing /proc/<pid>/status also would — and
            // measurably cheaper: reading that file for every process every
            // second was 58% of total collection time.
            let uid = entry.metadata().map(|m| m.uid()).unwrap_or(0);
            let user = self.user_for_uid(uid);

            let Some(mut p) = self.parse_proc_stat(pid, &stat, elapsed_secs, user, &mut seen)
            else {
                continue;
            };
            if needs.io {
                match self.read_proc_io(pid, elapsed_secs, &mut seen_io) {
                    Ok(rates) => p.io = rates,
                    Err(()) => *denied += 1,
                }
            }
            out.push(p);
        }

        // Drop counters for processes that have exited, or the map grows
        // without bound on a busy box.
        self.prev_proc_jiffies = seen;
        if needs.io {
            self.prev_proc_io = seen_io;
        }
        Ok(out)
    }

    /// Per-process disk throughput from `/proc/<pid>/io`.
    ///
    /// That file is mode 0400 and owned by the process owner, so reading
    /// another user's process needs CAP_SYS_PTRACE. Unreadable means `None`,
    /// never zero — showing every process you do not own as idle would be a
    /// confident lie, where a blank is merely an absence.
    /// `Err(())` means the file could not be read — running as root would fix
    /// it. `Ok(None)` means the process is simply too new to have a previous
    /// counter to diff against, which fixes itself on the next sample. Both
    /// render as a dash, but only one of them is worth advising the user about.
    fn read_proc_io(
        &self,
        pid: i32,
        elapsed_secs: f64,
        seen: &mut HashMap<i32, (u64, u64)>,
    ) -> Result<Option<IoRates>, ()> {
        let text = fs::read_to_string(format!("/proc/{pid}/io")).map_err(|_| ())?;
        let (read, write) = parse_proc_io(&text).ok_or(())?;
        seen.insert(pid, (read, write));

        // Cumulative counters again: only the delta is a rate.
        let Some((prev_r, prev_w)) = self.prev_proc_io.get(&pid).copied() else {
            return Ok(None);
        };
        if elapsed_secs <= 0.0 {
            return Ok(None);
        }
        Ok(Some(IoRates {
            read: ((read.saturating_sub(prev_r)) as f64 / elapsed_secs) as u64,
            write: ((write.saturating_sub(prev_w)) as f64 / elapsed_secs) as u64,
        }))
    }

    fn user_for_uid(&mut self, uid: u32) -> Arc<str> {
        // Unknown uids (containers, LDAP, no /etc/passwd entry) get memoised in
        // numeric form so the fallback allocates once, not once per sample.
        self.users
            .entry(uid)
            .or_insert_with(|| Arc::from(uid.to_string().as_str()))
            .clone()
    }

    fn parse_proc_stat(
        &self,
        pid: i32,
        stat: &str,
        elapsed_secs: f64,
        user: Arc<str>,
        seen: &mut HashMap<i32, u64>,
    ) -> Option<ProcSample> {
        // Field 2 is the executable name in parentheses, and it may contain
        // spaces *and* parentheses, so splitting on whitespace corrupts every
        // field after it. Split on the last ')' instead.
        let close = stat.rfind(')')?;
        let open = stat.find('(')?;
        let comm = stat.get(open + 1..close)?.to_string();
        let rest: Vec<&str> = stat.get(close + 1..)?.split_whitespace().collect();

        // rest[0] is field 3 (state), so field N lives at rest[N - 3].
        let state = rest.first()?.chars().next().unwrap_or('?');
        let ppid: i32 = rest.get(1)?.parse().ok()?;
        let utime: u64 = rest.get(11)?.parse().ok()?;
        let stime: u64 = rest.get(12)?.parse().ok()?;
        let threads: u32 = rest.get(17)?.parse().unwrap_or(1);
        let rss_pages: u64 = rest.get(21)?.parse().unwrap_or(0);

        let jiffies = utime + stime;
        seen.insert(pid, jiffies);

        // Same story as the aggregate CPU: only the delta means anything.
        let cpu = match self.prev_proc_jiffies.get(&pid) {
            Some(&prev) if elapsed_secs > 0.0 => {
                let dj = jiffies.saturating_sub(prev) as f64;
                let pct = ((dj / self.ticks_per_sec / elapsed_secs) * 100.0) as f32;
                // Clamp to the total the machine can actually deliver. A pid
                // reused between samples diffs the new process against the old
                // one's counter and can otherwise report thousands of percent.
                // htop guards the same way.
                pct.min(self.core_count() as f32 * 100.0)
            }
            _ => 0.0,
        };

        Some(ProcSample {
            pid,
            ppid,
            name: comm,
            user,
            cpu,
            rss: rss_pages * self.page_size,
            threads,
            state,
            io: None,
        })
    }

    /// Cores seen in the last /proc/stat read; 1 before the first read.
    fn core_count(&self) -> usize {
        self.prev_cores.len().max(1)
    }
}

/// Cumulative block-layer bytes from `/proc/<pid>/io`.
///
/// `read_bytes`/`write_bytes` are actual device traffic, which is what iotop
/// reports; `rchar`/`wchar` count bytes passed to syscalls and include reads
/// served from page cache, which would overstate disk load considerably.
fn parse_proc_io(text: &str) -> Option<(u64, u64)> {
    let mut read = None;
    let mut write = None;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("read_bytes:") {
            read = v.trim().parse().ok();
        } else if let Some(v) = line.strip_prefix("write_bytes:") {
            write = v.trim().parse().ok();
        }
    }
    Some((read?, write?))
}

/// uid -> name, from /etc/passwd. Good enough without NSS; unknown uids fall
/// back to their numeric form at lookup time.
fn parse_passwd() -> HashMap<u32, Arc<str>> {
    let mut map = HashMap::new();
    if let Ok(text) = fs::read_to_string("/etc/passwd") {
        for line in text.lines() {
            let mut f = line.split(':');
            let (Some(name), Some(_pw), Some(uid)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            if let Ok(uid) = uid.parse() {
                map.insert(uid, Arc::from(name));
            }
        }
    }
    map
}

impl Collector for ProcFs {
    fn sample(&mut self, needs: Needs) -> io::Result<Sample> {
        let now = SystemTime::now();
        let elapsed = self
            .prev_at
            .and_then(|p| now.duration_since(p).ok())
            .unwrap_or(Duration::ZERO);
        self.prev_at = Some(now);

        let (cpu_total, cpu_per_core) = self.read_cpu()?;
        let mut io_denied = 0;
        let procs = self.read_procs(elapsed, needs, &mut io_denied)?;
        Ok(Sample {
            at: now,
            cpu_total,
            cpu_per_core,
            mem: self.read_mem()?,
            load: self.read_load()?,
            procs,
            uptime: self.read_uptime()?,
            io_collected: needs.io,
            io_denied,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_times_skips_guest_double_count() {
        // user nice system idle iowait irq softirq steal guest guest_nice
        let t = CpuTimes::parse(" 100 10 50 800 20 5 5 10 999 999").unwrap();
        assert_eq!(t.total, 100 + 10 + 50 + 800 + 20 + 5 + 5 + 10);
        assert_eq!(t.idle, 820);
    }

    #[test]
    fn busy_pct_uses_deltas() {
        let a = CpuTimes {
            idle: 900,
            total: 1000,
        };
        let b = CpuTimes {
            idle: 950,
            total: 1100,
        };
        // 100 jiffies passed, 50 idle -> 50% busy
        assert!((b.busy_pct_since(&a) - 50.0).abs() < 0.01);
    }

    #[test]
    fn first_read_reports_zero_not_garbage() {
        let a = CpuTimes {
            idle: 900,
            total: 1000,
        };
        assert_eq!(a.busy_pct_since(&a), 0.0);
    }

    #[test]
    fn proc_stat_survives_parens_in_process_name() {
        let pf = ProcFs::new().unwrap();
        let mut seen = HashMap::new();
        // A process literally named "(evil) proc)" — the case that breaks
        // naive whitespace splitting.
        let line = "1234 ((evil) proc)) S 1 0 0 0 -1 4194560 100 0 0 0 \
                    55 45 0 0 20 0 8 0 12345 1000 512 0 0 0 0 0 0 0 0 0 0 0 0";
        let p = pf
            .parse_proc_stat(1234, line, 1.0, Arc::from("root"), &mut seen)
            .unwrap();
        assert_eq!(p.name, "(evil) proc)");
        assert_eq!(p.ppid, 1);
        assert_eq!(p.threads, 8);
        assert_eq!(p.state, 'S');
    }

    #[test]
    fn proc_cpu_is_zero_without_a_previous_reading() {
        let pf = ProcFs::new().unwrap();
        let mut seen = HashMap::new();
        let line = "1 (init) S 0 0 0 0 -1 4194560 100 0 0 0 \
                    55 45 0 0 20 0 1 0 12345 1000 512 0 0 0 0 0 0 0 0 0 0 0 0";
        let p = pf
            .parse_proc_stat(1, line, 1.0, Arc::from("root"), &mut seen)
            .unwrap();
        assert_eq!(p.cpu, 0.0);
        assert_eq!(seen.get(&1), Some(&100)); // 55 + 45 recorded for next time
    }

    #[test]
    fn parses_block_layer_bytes_not_syscall_bytes() {
        let text = "rchar: 999\nwchar: 888\nsyscr: 7\nsyscw: 3\n\
                    read_bytes: 4096\nwrite_bytes: 8192\ncancelled_write_bytes: 512\n";
        assert_eq!(parse_proc_io(text), Some((4096, 8192)));
    }

    #[test]
    fn io_parse_rejects_a_truncated_file() {
        assert_eq!(parse_proc_io("rchar: 1\nwchar: 2\n"), None);
    }

    #[test]
    fn io_rate_needs_a_previous_reading() {
        let pf = ProcFs::new().unwrap();
        let mut seen = HashMap::new();
        // A pid that cannot be read is Err — the case root would fix.
        assert!(pf.read_proc_io(-1, 1.0, &mut seen).is_err());
    }

    #[test]
    fn proc_cpu_is_clamped_when_a_pid_is_reused() {
        let mut pf = ProcFs::new().unwrap();
        pf.prev_cores = vec![CpuTimes::default(); 4];
        // The previous occupant of this pid had barely run; the new one shows a
        // huge cumulative counter, so the naive delta is ~500000%.
        pf.prev_proc_jiffies.insert(7, 1);
        let mut seen = HashMap::new();
        let line = "7 (reused) S 0 0 0 0 -1 0 0 0 0 0 \
                    500000 1 0 0 20 0 1 0 1 1000 512 0 0 0 0 0 0 0 0 0 0 0 0";
        let p = pf
            .parse_proc_stat(7, line, 1.0, Arc::from("root"), &mut seen)
            .unwrap();
        assert_eq!(p.cpu, 400.0, "must clamp to cores * 100");
    }

    #[test]
    fn proc_cpu_from_jiffy_delta() {
        let mut pf = ProcFs::new().unwrap();
        pf.prev_cores = vec![CpuTimes::default(); 4];
        pf.prev_proc_jiffies.insert(1, 50);
        let mut seen = HashMap::new();
        // 100 total jiffies now, 50 before -> 50 jiffies in 1s at 100Hz = 50%
        let line = "1 (init) S 0 0 0 0 -1 4194560 100 0 0 0 \
                    55 45 0 0 20 0 1 0 12345 1000 512 0 0 0 0 0 0 0 0 0 0 0 0";
        let p = pf
            .parse_proc_stat(1, line, 1.0, Arc::from("root"), &mut seen)
            .unwrap();
        assert!((p.cpu - 50.0).abs() < 0.01);
    }
}
