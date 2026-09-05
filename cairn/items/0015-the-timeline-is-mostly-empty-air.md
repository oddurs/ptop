---
id: 15
title: The timeline is mostly empty air
type: bug
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p0
effort: m
area: ui
---

## Problem

A fixed 0–100 axis means a machine at 16% CPU lights the bottom row and leaves
eight rows blank. On a tall terminal the timeline is the largest panel on
screen and most of it is nothing.

Worse, the threshold rules are dashed across all that emptiness, so at 200
columns they are a hundred dots of noise that read as screen dust rather than
as reference lines.

## Proposal

Scale the graph to the peak in the visible window, rounded up to a readable
ceiling — 10, 25, 50, 100 — and label the axis with it. btop does this and it
is why btop's graphs look full.

Nothing becomes misleading because the ceiling is printed: the axis says what
it is. Comparability across scrubbing is preserved because the ceiling only
moves between those fixed steps rather than tracking the peak continuously.

The threshold rules then draw only when the threshold is inside the visible
range, which removes the dust in the common case where nothing is near 80%.

## Acceptance criteria

- [ ] Graph fills its rows for any workload, idle or saturated
- [ ] The ceiling is labelled, and is one of a small set of steps
- [ ] Scrubbing does not make the ceiling flicker between adjacent samples
- [ ] Threshold rules appear only when the threshold is on the visible scale
- [ ] A saturated machine still reads 0–100
