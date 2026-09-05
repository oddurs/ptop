//! Application state and input handling.

use crate::glyphs::GlyphSet;
use crate::history::History;
use crate::sample::{ProcSample, Sample};

/// Samples per display slot.
///
/// Kept modest deliberately: a slot count of ~200 (braille on a normal
/// terminal) needs only 3-4 samples per slot to cover the whole ten-minute
/// buffer, and a narrow terminal needs ~8. Offering 30 would just give three
/// keypresses that visibly do nothing, since [`effective_zoom`] clamps to what
/// the buffer can actually fill.
pub const ZOOM_LEVELS: [usize; 4] = [1, 2, 4, 8];

/// The zoom actually used to draw, given how much history exists.
///
/// Zooming past the point where the whole buffer is on screen only shrinks the
/// data into a corner, so it is clamped. The empty region to the left of a
/// fully zoomed-out graph is meaningful — it is time from before the buffer
/// starts, not missing data.
pub fn effective_zoom(requested: usize, samples: usize, slots: usize) -> usize {
    if slots == 0 {
        return requested.max(1);
    }
    requested.max(1).min(samples.div_ceil(slots).max(1))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Sort {
    Cpu,
    Mem,
    Pid,
    Name,
}

impl Sort {
    pub fn label(self) -> &'static str {
        match self {
            Sort::Cpu => "CPU",
            Sort::Mem => "MEM",
            Sort::Pid => "PID",
            Sort::Name => "NAME",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Sort::Cpu => Sort::Mem,
            Sort::Mem => Sort::Pid,
            Sort::Pid => Sort::Name,
            Sort::Name => Sort::Cpu,
        }
    }
}

pub struct App {
    pub history: History,
    pub sort: Sort,
    /// Index into the *sorted* process list of the displayed sample.
    pub selected: usize,
    pub filter: String,
    pub editing_filter: bool,
    pub should_quit: bool,
    /// Index into [`ZOOM_LEVELS`].
    zoom_idx: usize,
    pub glyphs: GlyphSet,
}

impl App {
    pub fn new(history_len: usize) -> Self {
        Self {
            history: History::new(history_len),
            sort: Sort::Cpu,
            selected: 0,
            filter: String::new(),
            editing_filter: false,
            should_quit: false,
            zoom_idx: 0,
            glyphs: GlyphSet::default(),
        }
    }

    /// Samples per display slot.
    pub fn zoom(&self) -> usize {
        ZOOM_LEVELS[self.zoom_idx]
    }

    /// Zoom out: more time on screen, coarser slots.
    pub fn zoom_out(&mut self) {
        self.zoom_idx = (self.zoom_idx + 1).min(ZOOM_LEVELS.len() - 1);
    }

    /// Zoom in: less time on screen, one slot per sample at the limit.
    pub fn zoom_in(&mut self) {
        self.zoom_idx = self.zoom_idx.saturating_sub(1);
    }

    pub fn push(&mut self, s: Sample) {
        self.history.push(s);
    }

    /// Processes of the displayed sample, filtered and sorted for the table.
    pub fn visible_procs(&self) -> Vec<&ProcSample> {
        let Some(sample) = self.history.current() else {
            return Vec::new();
        };
        let needle = self.filter.to_lowercase();
        let mut v: Vec<&ProcSample> = sample
            .procs
            .iter()
            .filter(|p| {
                needle.is_empty()
                    || p.name.to_lowercase().contains(&needle)
                    || p.pid.to_string().contains(&needle)
            })
            .collect();

        match self.sort {
            // Descending for the resource columns: the interesting rows go top.
            Sort::Cpu => v.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
            Sort::Mem => v.sort_by_key(|p| std::cmp::Reverse(p.rss)),
            Sort::Pid => v.sort_by_key(|p| p.pid),
            Sort::Name => v.sort_by_key(|p| p.name.to_lowercase()),
        }
        v
    }

    pub fn select_delta(&mut self, delta: isize) {
        let n = self.visible_procs().len();
        if n == 0 {
            self.selected = 0;
            return;
        }
        let next = self.selected as isize + delta;
        self.selected = next.clamp(0, n as isize - 1) as usize;
    }

    /// Keep the selection in range after the list shrinks (filter, or a process
    /// exiting between samples).
    pub fn clamp_selection(&mut self) {
        let n = self.visible_procs().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }
}
