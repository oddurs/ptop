//! Application state and input handling.

use crate::history::History;
use crate::sample::{ProcSample, Sample};

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
        }
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
