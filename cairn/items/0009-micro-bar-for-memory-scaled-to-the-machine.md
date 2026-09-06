---
id: 9
title: Micro-bar for memory, scaled to the machine
type: feature
status: done
milestone: v0.1
created: 2026-09-05
updated: 2026-09-05
priority: p3
effort: s
area: table
---

## Problem

`RSS` is an absolute byte figure and gives no sense of proportion. `2.1G` means
something quite different on an 8G box and a 512G one.

## Proposal

The same treatment as the CPU column, scaled against total system memory.

Scaled against the **displayed** sample's `mem.total`, not the live one, so it
stays correct while scrubbed. Everything else in the table follows the cursor;
this must too.

## Acceptance criteria

- [ ] Scaled from the displayed sample, verified while scrubbed
- [ ] Consistent with the CPU bar's visual language
