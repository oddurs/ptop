//! Fixed-capacity ring buffer of samples, plus the cursor that walks it.
//!
//! This is the heart of ptop. A conventional monitor renders the latest
//! snapshot and throws it away; here every sample is retained for the length of
//! the window, and the UI renders whatever the cursor points at. "Live" is just
//! the cursor sitting on the newest sample.

use crate::sample::Sample;
use std::collections::VecDeque;

pub struct History {
    samples: VecDeque<Sample>,
    capacity: usize,
    /// Index into `samples`, or `None` when tailing the live edge.
    ///
    /// Storing `None` rather than "the last index" means pushes don't have to
    /// fix up the cursor, and there is exactly one representation of "live".
    cursor: Option<usize>,
}

impl History {
    pub fn new(capacity: usize) -> Self {
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            cursor: None,
        }
    }

    pub fn push(&mut self, s: Sample) {
        if self.samples.len() == self.capacity {
            self.samples.pop_front();
            // The window slid by one, so a pinned cursor must slide too or it
            // would silently drift forward through time.
            if let Some(c) = self.cursor.as_mut() {
                *c = c.saturating_sub(1);
            }
        }
        self.samples.push_back(s);
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// True when the cursor is tracking the newest sample.
    pub fn is_live(&self) -> bool {
        self.cursor.is_none()
    }

    /// The sample currently being displayed.
    pub fn current(&self) -> Option<&Sample> {
        match self.cursor {
            Some(i) => self.samples.get(i),
            None => self.samples.back(),
        }
    }

    /// Index of the displayed sample, for drawing the timeline cursor.
    pub fn cursor_index(&self) -> usize {
        self.cursor.unwrap_or(self.samples.len().saturating_sub(1))
    }

    /// How far behind live the cursor is in wall-clock time.
    ///
    /// Read from the sample timestamps rather than multiplying the sample
    /// count by the interval: collection can stall under load, and a lag
    /// figure that quietly drifts from reality is worse than none.
    pub fn time_behind(&self) -> std::time::Duration {
        let (Some(cur), Some(newest)) = (self.current(), self.samples.back()) else {
            return std::time::Duration::ZERO;
        };
        newest.at.duration_since(cur.at).unwrap_or_default()
    }

    /// Wall-clock time covered by the retained samples.
    ///
    /// Read from the timestamps, not from `len() * interval`. Those agree only
    /// on a machine that never slept and never fell behind — the two cases the
    /// timeline now draws a seam for. A buffer whose graph announces that time
    /// is missing must not caption itself with a duration that excludes it.
    pub fn span(&self) -> std::time::Duration {
        let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) else {
            return std::time::Duration::ZERO;
        };
        last.at.duration_since(first.at).unwrap_or_default()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Sample> {
        self.samples.iter()
    }

    /// Move the cursor back (negative) or forward (positive) in time.
    ///
    /// Scrubbing forward past the newest sample returns to live tailing rather
    /// than pinning to the last index, so the view resumes updating.
    pub fn scrub(&mut self, delta: isize) {
        if self.samples.is_empty() {
            return;
        }
        let last = self.samples.len() - 1;
        let from = self.cursor_index() as isize;
        let target = from + delta;

        if target >= last as isize {
            self.cursor = None;
        } else {
            self.cursor = Some(target.max(0) as usize);
        }
    }

    /// Jump to the oldest retained sample.
    pub fn goto_oldest(&mut self) {
        if !self.samples.is_empty() {
            self.cursor = Some(0);
        }
    }

    /// Resume live tailing.
    pub fn goto_live(&mut self) {
        self.cursor = None;
    }
}

/// CPU history for a set of processes, over the newest `window` samples.
///
/// Returns one series per requested key, aligned oldest-first, with `None`
/// wherever the process was not present in that sample — a process that has
/// only just started leaves a gap rather than a run of zeroes, which would
/// read as "it was here and idle".
///
/// Keyed on pid **and** start time. On pid alone a recycled pid splices two
/// unrelated processes into one line, which is the same trap the name cache
/// had, with a more misleading result: a graph of two different programs.
///
/// One pass over the window, checking each process against the requested set,
/// rather than a scan per process per sample. The set is the visible rows, so
/// it is bounded by the terminal height however many processes the machine has.
pub fn series_for(
    history: &History,
    keys: &[(i32, u64)],
    window: usize,
) -> std::collections::HashMap<(i32, u64), Vec<Option<f32>>> {
    use std::collections::{HashMap, HashSet};
    let wanted: HashSet<(i32, u64)> = keys.iter().copied().collect();
    let mut out: HashMap<(i32, u64), Vec<Option<f32>>> = keys
        .iter()
        .map(|&k| (k, Vec::with_capacity(window)))
        .collect();

    let skip = history.len().saturating_sub(window);
    for sample in history.iter().skip(skip) {
        // Start every series with a gap, then fill the ones this sample has.
        for series in out.values_mut() {
            series.push(None);
        }
        for p in &sample.procs {
            let key = (p.pid, p.started);
            if wanted.contains(&key)
                && let Some(series) = out.get_mut(&key)
                && let Some(last) = series.last_mut()
            {
                *last = Some(p.cpu);
            }
        }
    }
    out
}

/// Aggregate the newest values into exactly `slots` display slots.
///
/// Right-aligned on purpose: the newest value in `values` always lands in the
/// last slot, so slot boundaries do not shift under the viewer every time a
/// sample arrives. Note "newest in `values`", not "newest overall" — the caller
/// chooses the slice, and since G7 it scrolls that slice to follow the cursor.
/// Leading slots with nothing to show are `None` rather than zero, so an
/// unfilled buffer reads as empty instead of as an idle machine.
///
/// Each slot takes the **peak** of the samples it covers, never the mean.
/// Averaging a 100% spike with three idle samples renders 25% and hides exactly
/// the event this tool exists to catch.
pub fn peak_slots(values: &[f32], zoom: usize, slots: usize) -> Vec<Option<f32>> {
    let zoom = zoom.max(1);
    let n = values.len();
    let mut out = vec![None; slots];

    for (k, slot) in out.iter_mut().rev().enumerate() {
        // Slot k back from the right covers the k-th block of `zoom` values,
        // counting back from the newest.
        let end = n.saturating_sub(k * zoom);
        if end == 0 {
            break;
        }
        let start = end.saturating_sub(zoom);
        *slot = values[start..end]
            .iter()
            .copied()
            .fold(None::<f32>, |acc, v| Some(acc.map_or(v, |a: f32| a.max(v))));
    }
    out
}

/// Which samples are not contiguous in time with the one before them.
///
/// A laptop that sleeps, or a box loaded enough to miss its tick, produces
/// samples minutes apart. Rendered as adjacent cells they claim to be one
/// interval apart, and the x-axis quietly stops meaning anything — the graph
/// compresses twenty minutes of absence into the same width as one second of
/// idle. htop carries a comment about this exact hazard ("period might be 0
/// after system sleep"), which is somebody else's scar tissue, available free.
///
/// A gap is defined as **a missing sample**, not a slow one: at least twice the
/// nominal interval means at least one tick went unobserved. Collection jitter
/// under load stretches an interval by a fraction, never doubles it, so the
/// line separates the two without a tuning knob.
///
/// The flag marks the sample *after* the discontinuity — the one whose arrival
/// is unaccounted for. Index 0 is never a gap: it has no predecessor here, and
/// inventing one would put a seam at the left edge of every fresh buffer.
pub fn gaps_in(times: &[std::time::SystemTime], nominal: std::time::Duration) -> Vec<bool> {
    // A zero nominal interval has no notion of a missed tick, and `>= 0` would
    // otherwise flag every sample and render the whole graph as seams. Item
    // 0013 makes this number configurable, so the degenerate value stops being
    // hypothetical the moment someone writes `interval = 0`.
    if nominal.is_zero() {
        return vec![false; times.len()];
    }
    // Floored, because "one missed tick" stops being a meaningful statement as
    // the interval shrinks. At `interval = 50ms` a frame that took 100ms to
    // draw and collect would otherwise read as time missing, and a monitor
    // that is merely busy would paint itself full of seams. Below a quarter of
    // a second there is no gap worth telling anyone about.
    const FLOOR: std::time::Duration = std::time::Duration::from_millis(250);
    let limit = nominal.saturating_mul(2).max(FLOOR);
    times
        .iter()
        .enumerate()
        .map(
            |(i, at)| match i.checked_sub(1).and_then(|j| times.get(j)) {
                // A clock that went backwards is not a gap. `duration_since` fails
                // rather than reporting it, and treating that as a gap would paint
                // seams across the whole graph on a machine that just stepped NTP.
                Some(prev) => at.duration_since(*prev).is_ok_and(|d| d >= limit),
                None => false,
            },
        )
        .collect()
}

/// Pack per-sample flags into display slots, the same right-aligned way
/// [`peak_slots`] packs values.
///
/// Aggregated by **or**, which is the boolean form of the same rule that makes
/// values aggregate by peak: zooming out must not be able to erase an event.
/// Any other rule would let a gap vanish at the zoom level where the whole
/// buffer is on screen — precisely the view you would be in to notice one.
pub fn any_slots(flags: &[bool], zoom: usize, slots: usize) -> Vec<bool> {
    let zoom = zoom.max(1);
    let n = flags.len();
    let mut out = vec![false; slots];

    for (k, slot) in out.iter_mut().rev().enumerate() {
        let end = n.saturating_sub(k * zoom);
        if end == 0 {
            break;
        }
        let start = end.saturating_sub(zoom);
        *slot = flags[start..end].iter().any(|&f| f);
    }
    out
}

/// Which display slot holds the sample at `index` within a window of
/// `n_values`, under the same right-aligned packing as [`peak_slots`].
pub fn slot_of_index(index: usize, n_values: usize, zoom: usize, slots: usize) -> usize {
    let zoom = zoom.max(1);
    let from_newest = n_values.saturating_sub(1).saturating_sub(index);
    slots.saturating_sub(1).saturating_sub(from_newest / zoom)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_with_cpu(cpu: f32) -> Sample {
        let mut s = Sample::empty();
        s.cpu_total = cpu;
        s
    }

    #[test]
    fn starts_live_and_empty() {
        let h = History::new(4);
        assert!(h.is_live());
        assert!(h.current().is_none());
    }

    #[test]
    fn live_cursor_follows_newest() {
        let mut h = History::new(4);
        h.push(sample_with_cpu(1.0));
        h.push(sample_with_cpu(2.0));
        assert_eq!(h.current().unwrap().cpu_total, 2.0);
    }

    #[test]
    fn scrub_back_pins_to_older_sample() {
        let mut h = History::new(8);
        for i in 0..5 {
            h.push(sample_with_cpu(i as f32));
        }
        h.scrub(-2);
        assert!(!h.is_live());
        assert_eq!(h.current().unwrap().cpu_total, 2.0);
        assert_eq!(h.cursor_index(), 2);
    }

    #[test]
    fn pinned_cursor_holds_its_sample_as_window_slides() {
        let mut h = History::new(3);
        h.push(sample_with_cpu(1.0));
        h.push(sample_with_cpu(2.0));
        h.push(sample_with_cpu(3.0));
        h.scrub(-2); // pinned to the 1.0 sample
        assert_eq!(h.current().unwrap().cpu_total, 1.0);

        // Evicting 1.0 slides the cursor down; it must not jump forward in time.
        h.push(sample_with_cpu(4.0));
        assert_eq!(h.current().unwrap().cpu_total, 2.0);
    }

    #[test]
    fn scrub_forward_past_end_resumes_live() {
        let mut h = History::new(8);
        for i in 0..4 {
            h.push(sample_with_cpu(i as f32));
        }
        h.scrub(-3);
        assert!(!h.is_live());
        h.scrub(99);
        assert!(h.is_live());
        h.push(sample_with_cpu(9.0));
        assert_eq!(h.current().unwrap().cpu_total, 9.0);
    }

    #[test]
    fn scrub_back_past_start_clamps() {
        let mut h = History::new(8);
        for i in 0..3 {
            h.push(sample_with_cpu(i as f32));
        }
        h.scrub(-100);
        assert_eq!(h.current().unwrap().cpu_total, 0.0);
    }

    #[test]
    fn peak_slots_takes_the_peak_not_the_mean() {
        // A lone spike among idle samples must survive aggregation.
        let v = [0.0, 0.0, 100.0, 0.0];
        assert_eq!(peak_slots(&v, 4, 1), vec![Some(100.0)]);
    }

    #[test]
    fn peak_slots_is_right_aligned() {
        let v = [1.0, 2.0, 3.0];
        // Newest value lands in the last slot; the unfilled slot stays None.
        assert_eq!(
            peak_slots(&v, 1, 5),
            vec![None, None, Some(1.0), Some(2.0), Some(3.0)]
        );
    }

    #[test]
    fn peak_slots_alignment_is_stable_as_samples_arrive() {
        // The newest sample must stay pinned to the right edge, otherwise the
        // whole graph shuffles sideways once per second.
        let a = peak_slots(&[1.0, 2.0, 3.0, 4.0], 2, 4);
        let b = peak_slots(&[0.0, 1.0, 2.0, 3.0, 4.0], 2, 4);
        assert_eq!(a.last(), b.last());
        assert_eq!(a.last(), Some(&Some(4.0)));
    }

    #[test]
    fn peak_slots_groups_by_zoom() {
        let v = [1.0, 9.0, 2.0, 8.0];
        assert_eq!(peak_slots(&v, 2, 2), vec![Some(9.0), Some(8.0)]);
    }

    #[test]
    fn peak_slots_handles_empty_input() {
        assert_eq!(peak_slots(&[], 3, 2), vec![None, None]);
    }

    #[test]
    fn peak_slots_drops_values_that_do_not_fit() {
        // More values than slots can hold: the oldest fall off the left, and
        // the newest are the ones kept.
        let v = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(peak_slots(&v, 1, 2), vec![Some(3.0), Some(4.0)]);
    }

    #[test]
    fn slot_of_index_tracks_the_packing() {
        // 4 values, zoom 1, 4 slots: one slot each.
        assert_eq!(slot_of_index(3, 4, 1, 4), 3);
        assert_eq!(slot_of_index(0, 4, 1, 4), 0);
        // zoom 2: values 2 and 3 share the last slot.
        assert_eq!(slot_of_index(3, 4, 2, 2), 1);
        assert_eq!(slot_of_index(2, 4, 2, 2), 1);
        assert_eq!(slot_of_index(1, 4, 2, 2), 0);
    }

    #[test]
    fn capacity_is_enforced() {
        let mut h = History::new(3);
        for i in 0..10 {
            h.push(sample_with_cpu(i as f32));
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.current().unwrap().cpu_total, 9.0);
    }
}
