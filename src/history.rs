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
    fn capacity_is_enforced() {
        let mut h = History::new(3);
        for i in 0..10 {
            h.push(sample_with_cpu(i as f32));
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.current().unwrap().cpu_total, 9.0);
    }
}
