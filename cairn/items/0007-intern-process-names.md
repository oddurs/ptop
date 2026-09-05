---
id: 7
title: Intern process names
type: feature
status: done
milestone: v0.2
created: 2026-09-05
updated: 2026-09-05
priority: p1
effort: s
area: collect
---

## Problem

The name is re-parsed and re-allocated for every process on every sample. At 400
processes over a 600-sample buffer that is **240,000 String allocations
retained, 3.8 MB**, for strings that essentially never change.

`user` is already interned as `Arc<str>` — the fix that made the uid lookup
2.26× faster. `name` never got the same treatment. htop reads `comm`/`cmdline`
once per process *lifetime* (`!preExisting`); btop gates the same reads on
`no_cache`. ptop has no such gate.

## Proposal

`ProcSample::name` becomes `Arc<str>`, cached per pid, refreshed only when the
pid is new to the collector.

**The wrinkle that makes this a correctness problem, not an optimisation.** A
pid can be reused, and the new process usually has a different name. Keyed on
pid alone, a recycled pid inherits the dead process's name. `/proc/<pid>/stat`
field 22 is the start time and is already parsed past; pid plus start time is a
key that survives reuse.

## Acceptance criteria

- [ ] Allocations per sample drop from one per process to one per *new* process
- [ ] Retained memory falls by roughly 3.8 MB at 400 procs × 600 samples
- [ ] A pid reused inside the buffer window does not inherit the old name — tested directly
