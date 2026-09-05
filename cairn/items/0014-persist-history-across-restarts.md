---
id: 14
title: Persist history across restarts
type: feature
status: backlog
milestone: later
created: 2026-09-05
updated: 2026-09-05
priority: p3
effort: l
area: collect
---

## Problem

Quitting ptop discards everything. atop and zenith both survive a restart.

## Proposal

Optionally write samples to disk and reload on start.

**This changes what ptop is** — from a live tool with a memory into a small
recorder — so it needs its own design pass covering retention, on-disk format,
file size, and whether it implies a daemon.

The zero-setup argument in the README depends on ptop **not** requiring one.
"Nothing has to have been running beforehand" is the whole position against
atop, so this must stay opt-in and must not become the default path.

## Acceptance criteria

- [ ] Off by default
- [ ] Bounded on-disk size
- [ ] Versioned format
- [ ] Startup cost with a full store is measured
- [ ] Zero-setup behaviour is unchanged when off
