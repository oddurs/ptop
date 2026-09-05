//! Core data model.
//!
//! Everything the collectors produce is a `Sample`: a complete, self-contained
//! snapshot of the machine at one instant, including the full process list.
//! Keeping processes *inside* the sample is what makes scrubbing backwards
//! possible — the process table you see at t-40s is the real one from t-40s,
//! not an interpolation.

use std::time::SystemTime;

/// Memory figures, all in bytes.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemStat {
    pub total: u64,
    /// Memory actually in use (total minus available), the number people mean
    /// when they ask "how much RAM is this box using".
    pub used: u64,
    pub available: u64,
    pub swap_total: u64,
    pub swap_used: u64,
}

impl MemStat {
    pub fn used_pct(&self) -> f32 {
        if self.total == 0 {
            return 0.0;
        }
        (self.used as f32 / self.total as f32) * 100.0
    }

    pub fn swap_pct(&self) -> f32 {
        if self.swap_total == 0 {
            return 0.0;
        }
        (self.swap_used as f32 / self.swap_total as f32) * 100.0
    }
}

/// One process as it appeared in a single sample.
#[derive(Debug, Clone)]
pub struct ProcSample {
    pub pid: i32,
    /// Retained for the process-tree view; nothing reads it yet.
    #[allow(dead_code)]
    pub ppid: i32,
    pub name: String,
    pub user: String,
    /// Percent of one core. Can exceed 100 for threaded processes.
    pub cpu: f32,
    /// Resident set size in bytes.
    pub rss: u64,
    pub threads: u32,
    pub state: char,
}

/// A complete snapshot of the machine at one instant.
#[derive(Debug, Clone)]
pub struct Sample {
    pub at: SystemTime,
    /// Aggregate CPU busy percentage, 0..100.
    pub cpu_total: f32,
    /// Per-core busy percentage, 0..100 each.
    pub cpu_per_core: Vec<f32>,
    pub mem: MemStat,
    pub load: [f64; 3],
    pub procs: Vec<ProcSample>,
    pub uptime: std::time::Duration,
}

impl Sample {
    /// A zeroed sample. Test fixture only — the real path always starts from
    /// a genuine collection.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            at: SystemTime::now(),
            cpu_total: 0.0,
            cpu_per_core: Vec::new(),
            mem: MemStat::default(),
            load: [0.0; 3],
            procs: Vec::new(),
            uptime: std::time::Duration::ZERO,
        }
    }
}
