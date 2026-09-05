---
id: 13
title: Configurable sample interval and window
type: feature
status: backlog
milestone: v0.2
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: s
area: config
---

## Problem

`HISTORY_LEN` is a hardcoded 600 samples and the interval a hardcoded 1s. Both
are reasonable defaults and neither should be the only option.

## Proposal

`interval` and `window` in the config file, the window expressed in **time**
rather than a sample count.

Sub-second sampling narrows the window in which a short-lived process is
invisible; longer intervals extend the retained span on a quiet box. Both are
useful, and the ring buffer already tolerates uneven intervals because the
timestamps are real.

Depends on the config file, and on the collector being cheap enough that a
faster rate is not itself the problem — 0.14% of a core at 1 Hz becomes 1.44% at
10 Hz, and 14.4% at 10 Hz on a 4000-process box.

## Acceptance criteria

- [ ] Window and interval independently configurable
- [ ] Memory implications documented at the extremes
- [ ] Titles and lag figures report real time, not sample counts — they already do
