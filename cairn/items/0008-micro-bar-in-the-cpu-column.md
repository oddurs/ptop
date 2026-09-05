---
id: 8
title: Micro-bar in the CPU column
type: feature
status: backlog
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: s
area: table
---

## Problem

The process table is pure text, so finding the heavy process means reading
numbers rather than seeing them. htop puts a bar in its CPU column for exactly
this reason.

## Proposal

A short bar beside `CPU%` using the existing fractional block glyphs, scaled
0–100. Reuses `glyphs.rs`, so no new drawing code.

Values above 100% are real — a threaded process can exceed one core — and must
be visibly marked rather than silently clipped to full.

## Acceptance criteria

- [ ] The bar never widens the column beyond its budget
- [ ] Values over 100% are marked, not clipped
- [ ] Legible at the monochrome tier
