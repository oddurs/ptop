---
id: 16
title: Per-core meters render as an undifferentiated blob
type: bug
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p1
effort: s
area: ui
---

## Problem

One glyph per core with no separation means 14 cores render as a solid bar. It
reads as a single progress meter rather than fourteen independent ones, which
is the opposite of what it is for.

## Proposal

Group them, with a thin gap every four. `▂▄▁▃ ▅▂▁▁ ▃▆▂▁ ▂▄` is countable at a
glance; `▂▄▁▃▅▂▁▁▃▆▂▁▂▄` is not.

The grouping has to survive the overflow logic from 0002 — the count and the
`+N` marker still have to be exact once gaps take up width.

## Acceptance criteria

- [ ] Cores are visually countable in groups
- [ ] Overflow arithmetic still exact with gaps included
- [ ] 128 cores still legible on a wide terminal
