---
id: 12
title: Mark sampling gaps in the timeline
type: feature
status: done
milestone: v0.2
created: 2026-09-05
updated: 2026-09-06
priority: p2
effort: s
area: collect
---

## Problem

A laptop that sleeps, or a box under heavy load, produces samples minutes apart.
The timeline renders those adjacent cells as if they were one second apart,
silently compressing time.

htop carries a comment about this exact hazard — "period might be 0 after system
sleep" — which is somebody's scar tissue, available free.

## Proposal

Record the actual interval on each sample and draw a visible break where it
substantially exceeds the nominal one.

`History::time_behind` already reads real timestamps rather than counting rows;
this extends the same honesty to the graph itself.

## Acceptance criteria

- [ ] A gap is visible and distinguishable from an idle stretch
- [ ] Survives zoom aggregation — a gap must not be averaged away
