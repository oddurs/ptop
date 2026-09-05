//! Non-Linux backend, via `sysinfo`.
//!
//! There is no `/proc` here, and the mach calls that replace it are a different
//! project's worth of unsafe. This backend exists so the tool runs on a dev
//! laptop; the `/proc` backend is the one to read for how any of it works.

use super::Collector;
use crate::sample::{MemStat, ProcSample, Sample};
use std::io;
use std::time::{Duration, SystemTime};
use sysinfo::{ProcessesToUpdate, System, Users};

pub struct SysinfoCollector {
    sys: System,
    users: Users,
}

impl SysinfoCollector {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            sys: System::new_all(),
            users: Users::new_with_refreshed_list(),
        })
    }
}

impl Collector for SysinfoCollector {
    fn sample(&mut self) -> io::Result<Sample> {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);

        let cpu_per_core: Vec<f32> = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
        let cpu_total = if cpu_per_core.is_empty() {
            0.0
        } else {
            cpu_per_core.iter().sum::<f32>() / cpu_per_core.len() as f32
        };

        let procs = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcSample {
                pid: pid.as_u32() as i32,
                ppid: p.parent().map(|p| p.as_u32() as i32).unwrap_or(0),
                name: p.name().to_string_lossy().into_owned(),
                user: p
                    .user_id()
                    .and_then(|uid| self.users.get_user_by_id(uid))
                    .map(|u| std::sync::Arc::from(u.name()))
                    .unwrap_or_else(|| std::sync::Arc::from("?")),
                cpu: p.cpu_usage(),
                rss: p.memory(),
                // sysinfo exposes tasks only on Linux, where we use the other
                // backend anyway.
                threads: 1,
                state: status_char(p.status()),
            })
            .collect();

        let load = System::load_average();

        Ok(Sample {
            at: SystemTime::now(),
            cpu_total,
            cpu_per_core,
            mem: MemStat {
                total: self.sys.total_memory(),
                used: self.sys.used_memory(),
                available: self.sys.available_memory(),
                swap_total: self.sys.total_swap(),
                swap_used: self.sys.used_swap(),
            },
            load: [load.one, load.five, load.fifteen],
            procs,
            uptime: Duration::from_secs(System::uptime()),
        })
    }
}

/// Collapse to the single-letter states `ps` uses, so both backends agree.
fn status_char(s: sysinfo::ProcessStatus) -> char {
    use sysinfo::ProcessStatus::*;
    match s {
        Run => 'R',
        Sleep => 'S',
        Idle => 'I',
        Stop => 'T',
        Zombie => 'Z',
        _ => '?',
    }
}
