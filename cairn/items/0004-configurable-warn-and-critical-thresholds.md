---
id: 4
title: Configurable warn and critical thresholds
type: feature
status: backlog
milestone: v1.0
created: 2026-09-05
updated: 2026-09-05
priority: p3
effort: s
area: config
---

## Problem

`Theme::WARN_PCT` and `Theme::CRITICAL_PCT` are 50 and 80, compiled in. A build
box and a latency-sensitive service care at very different levels, and the
numbers already exist as a named pair.

## Proposal

    warn = 50
    critical = 80

in `ptop.conf`. They are already a single pair of constants, so the plumbing is
small — but three places read them and all must follow:

- the timeline's threshold rules (`G1`)
- `Theme::heat` and `figure_style`
- the scale legend in the header (`G6`), which prints them

`G6` has a test asserting the printed numbers are where the colour actually
changes. That test becomes more valuable once the numbers move, not less — it
is the thing that stops the legend drifting from the behaviour.

Ordering note: this should land **before** `D4`, which adds the interval
setting. Otherwise `D4` needs a second pass over the same config plumbing.

## Acceptance criteria

- [ ] Both thresholds settable from the config file
- [ ] Rules, heat and the legend all read the configured values
- [ ] `warn >= critical` is rejected with a message, not silently accepted
- [ ] Values outside 0..100 are rejected
- [ ] The G6 legend/behaviour agreement test passes at non-default thresholds
