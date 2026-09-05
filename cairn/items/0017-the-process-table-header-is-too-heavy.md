---
id: 17
title: The process table header is too heavy
type: bug
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p2
effort: s
area: ui
---

## Problem

A full-width reverse-video bar is the loudest thing on screen, and it is a
column header. It outweighs the data underneath it.

## Proposal

Bold and underlined rather than reversed. The row is still obviously a header
and stops competing with the processes.

Must stay legible at the monochrome tier, where reverse video was doing the
work.

## Acceptance criteria

- [ ] Header is distinguishable without reverse video
- [ ] Still distinguishable at the mono tier
