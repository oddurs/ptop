//! Application state and input handling.

use crate::collect::Needs;
use crate::glyphs::GlyphSet;
use crate::history::History;
use crate::sample::{ProcSample, Sample};
use crate::theme::Theme;
use crate::tree::{self, TreeRow};
use std::cmp::Ordering;
use std::collections::HashSet;

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

    /// Order two processes under this sort. Shared by the flat table and by
    /// sibling ordering inside the tree, so both agree.
    pub fn compare(self, a: &ProcSample, b: &ProcSample) -> Ordering {
        match self {
            // Descending for resource columns: the interesting rows go top.
            Sort::Cpu => b.cpu.total_cmp(&a.cpu),
            Sort::Mem => b.rss.cmp(&a.rss),
            Sort::Pid => a.pid.cmp(&b.pid),
            Sort::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
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
    pub tree: bool,
    /// Whether the IO columns are shown.
    pub show_io: bool,
    /// Whether IO is being collected. Deliberately a ratchet: hiding the
    /// columns does not stop collection, because resuming later would punch a
    /// hole in the middle of history. One clean boundary between "not collected
    /// yet" and "collected" is far easier to reason about while scrubbing than
    /// gaps wherever the column happened to be off.
    io_ratchet: bool,
    /// Index into [`ZOOM_LEVELS`].
    zoom_idx: usize,
    pub glyphs: GlyphSet,
    pub theme: Theme,
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
            tree: false,
            show_io: false,
            io_ratchet: false,
            zoom_idx: 0,
            glyphs: GlyphSet::default(),
            theme: Theme::default(),
        }
    }

    /// What the collector should gather for the next sample.
    pub fn needs(&self) -> Needs {
        Needs {
            io: self.io_ratchet,
        }
    }

    /// Show or hide the IO columns. Showing them starts collection; hiding them
    /// does not stop it. See [`App::io_ratchet`].
    pub fn toggle_io(&mut self) {
        self.show_io = !self.show_io;
        self.io_ratchet |= self.show_io;
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

    /// Rows of the displayed sample, filtered and ordered for the table.
    ///
    /// Flat and tree modes return the same row type so the renderer has one
    /// path; a flat row is simply one with an empty prefix.
    pub fn visible_rows(&self) -> Vec<TreeRow<'_>> {
        let Some(sample) = self.history.current() else {
            return Vec::new();
        };
        let needle = self.filter.to_lowercase();

        if self.tree {
            // A filtered tree keeps matches plus their ancestors; `tree::build`
            // works out the ancestry, so it only needs the direct matches.
            let matched: Option<HashSet<i32>> = (!needle.is_empty()).then(|| {
                sample
                    .procs
                    .iter()
                    .filter(|p| matches(p, &needle))
                    .map(|p| p.pid)
                    .collect()
            });
            return tree::build(&sample.procs, self.sort, matched.as_ref());
        }

        let mut v: Vec<&ProcSample> = sample
            .procs
            .iter()
            .filter(|p| matches(p, &needle))
            .collect();
        v.sort_by(|a, b| self.sort.compare(a, b));
        v.into_iter()
            .map(|p| TreeRow {
                proc: p,
                prefix: String::new(),
                context_only: false,
            })
            .collect()
    }

    pub fn select_delta(&mut self, delta: isize) {
        let n = self.visible_rows().len();
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
        let n = self.visible_rows().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }
}

/// A process matches the filter by name or by pid. An empty filter matches all.
fn matches(p: &ProcSample, needle: &str) -> bool {
    needle.is_empty()
        || p.name.to_lowercase().contains(needle)
        || p.pid.to_string().contains(needle)
}
