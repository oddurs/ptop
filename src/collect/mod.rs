//! Platform backends.
//!
//! The UI never learns which one it is talking to. On Linux we parse `/proc`
//! ourselves, which is where the interesting work is; on macOS there is no
//! `/proc`, so we lean on `sysinfo` to keep the tool runnable on a dev laptop.

use crate::sample::Sample;

pub trait Collector {
    /// Take one snapshot. Backends hold whatever raw counters they need to
    /// turn cumulative kernel numbers into per-interval rates.
    fn sample(&mut self) -> std::io::Result<Sample>;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::ProcFs as Platform;

#[cfg(not(target_os = "linux"))]
mod darwin;
#[cfg(not(target_os = "linux"))]
pub use darwin::SysinfoCollector as Platform;
