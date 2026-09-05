---
id: 11
title: Capture processes that live and die between samples
type: feature
status: backlog
milestone: v0.2
created: 2026-09-05
updated: 2026-09-05
priority: p0
effort: l
area: collect
---

## Problem

ptop samples once a second and reads `/proc` at that instant. A process that
lives 200ms never existed as far as ptop is concerned.

That is not an edge case for this tool. A burst of short-lived processes is one
of the most common causes of exactly the spike ptop exists to help you find — so
scrubbing back to the spike shows a process table that cannot explain the graph
above it.

atop solves this and is explicit about it: it reports "resource consumption by
all processes that were active during the interval, so also the resource
consumption by those processes that have finished during the interval". **This
is the substantive capability gap between the two tools**, and the reason the
README points people at atop for fleet history.

## Approaches, in preference order

1. **taskstats over netlink** — the kernel emits an exit record per process.
   Accurate. Needs `CAP_NET_ADMIN`. Linux only.
2. **BSD process accounting** (`acct(2)`) — needs root and a writable accounting
   file; more intrusive.
3. **Higher sample rate** — narrows the window without closing it. Cheap, a
   reasonable interim, and explicitly not a fix. Depends on the collector being
   cheap enough, which is why the E items come first.

## Degrade honestly

Without the capability, say so in the UI. Do not render an interval that
quietly omits what happened in it. Same principle as the gated IO columns:
never a fabricated zero.

## Acceptance criteria

- [ ] A process living 200ms appears in the interval containing it, marked as exited
- [ ] Missing capability is disclosed, not silent
- [ ] macOS degrades explicitly
- [ ] Sampling cost measured before and after with `--bench`
