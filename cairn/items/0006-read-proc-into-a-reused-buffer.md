---
id: 6
title: Read /proc into a reused buffer
type: feature
status: done
milestone: v0.2
created: 2026-09-05
updated: 2026-09-05
priority: p1
effort: m
area: collect
---

## Problem

Measured with `strace -c` at 304 processes, **per sample**: 2574 `read` (8.5 per
process), 802 `statx`, 484 `openat`.

`fs::read_to_string(format!("/proc/{pid}/stat"))` allocates a path String, then
does `openat` → `statx` → several `read`s → `close`. The `statx` exists to size
a buffer; `/proc` reports size 0, so it is pure waste — and the unknown size is
what forces the buffer to grow, which is where 8.5 reads per process comes from.

## Proposal

`openat` relative to a cached directory fd, one `read` into a buffer owned by
the collector, `close`. That is what htop does (`Compat_readfileat` →
`readfd_internal`) and btop is equivalent; neither stats anything to size a
read.

Sequenced before the D series: `D1` wants a faster sample rate to narrow the
window where a short-lived process is invisible, and the cost is linear in rate
— 0.14% of a core at 1 Hz today, 14.4% at 10 Hz on a 4000-process box.

## Acceptance criteria

- [ ] `statx` falls to about one per process — the uid `fstat`, which is needed
- [ ] `read` approaches one per file opened
- [ ] `--bench` improves; the number is recorded in `docs/roadmaps/06-collection-efficiency.md`
- [ ] No behaviour change: the `/proc` parsing tests are untouched and still pass
