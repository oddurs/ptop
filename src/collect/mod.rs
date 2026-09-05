//! Platform backends.
//!
//! The UI never learns which one it is talking to. On Linux we parse `/proc`
//! ourselves, which is where the interesting work is; on macOS there is no
//! `/proc`, so we lean on `sysinfo` to keep the tool runnable on a dev laptop.

use crate::sample::Sample;

/// Which optional, expensive data this sample should gather.
///
/// htop gates reads behind flags derived from the visible columns
/// (`PROCESS_FLAG_*`), so switching a column off stops the syscalls behind it.
/// Core figures — cpu, memory, rss, name, user, state, threads — are never
/// gated: the timeline and the default table depend on them, so their history
/// has to be complete.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Needs {
    /// Per-process disk throughput. One extra file read per process per sample.
    pub io: bool,
}

pub trait Collector {
    /// Take one snapshot. Backends hold whatever raw counters they need to
    /// turn cumulative kernel numbers into per-interval rates.
    fn sample(&mut self, needs: Needs) -> std::io::Result<Sample>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::ProcFs as Platform;

#[cfg(not(target_os = "linux"))]
mod darwin;
#[cfg(not(target_os = "linux"))]
pub use darwin::SysinfoCollector as Platform;
