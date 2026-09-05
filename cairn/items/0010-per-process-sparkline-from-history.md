---
id: 10
title: Per-process sparkline from history
type: feature
status: backlog
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p0
effort: m
area: table
---

## Problem

The ring buffer is a feature you have to go looking for. It is visible in the
timeline and nowhere else.

## Proposal

A small sparkline per row showing that process's CPU over the retained window,
drawn from history rather than from the current sample.

**This is the item worth building.** Every retained `Sample` carries its
complete `Vec<ProcSample>`, so ptop can already answer "what has *this process*
been doing for the last ten minutes" with no new collection — and no other
monitor can:

- htop, btop and bottom keep no per-process history at all
- zenith's history is aggregate-only; `HistogramKind` has no per-process variant
  and its table evicts dead pids each tick
- atop has the data but replays whole intervals from a logfile; it does not put
  a per-process trend beside the row in a live view

It also turns the ring buffer from a mode you must discover into something on
every row.

## Notes

- Reuse `History::peak_slots`. Peak, never mean — the same reason the timeline
  aggregates that way.
- Match a process across samples by pid **and** start time. Pid reuse would
  otherwise splice two unrelated processes into one line. Shares a key with the
  name interning item.
- Cost is per visible row, not per process, so it scales with the viewport.

## Acceptance criteria

- [ ] Covers the same window as the timeline and respects zoom
- [ ] Correct while scrubbed: shows history *up to the cursor*, not up to now,
      or it silently contradicts the table it sits in
- [ ] A pid reused inside the window does not produce a spliced line
- [ ] Render cost stays off the sample path — `--bench` unchanged
