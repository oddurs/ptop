# Process table

The table is pure text, so finding the heavy process means reading numbers
rather than seeing them. htop puts a bar in its CPU column for exactly this
reason.

`P3` is the important one in this file, and arguably in the whole roadmap: it
is a feature no other monitor can implement.

---

## P1 — Micro-bar in the CPU column  ·  `S`

**What.** A short bar rendered beside `CPU%` using the existing fractional block
glyphs, scaled 0–100 with overflow marked for multi-core processes.

**Why.** Turns a column that must be read into one that can be scanned. Reuses
`glyphs.rs`, so no new drawing code.

**Acceptance.** Bar never widens the column beyond its budget. Values above 100%
are visibly marked rather than clipped silently. Legible in monochrome.

**Touches.** `src/ui.rs` (`draw_procs`)

---

## P2 — Micro-bar for memory  ·  `S`

**What.** The same treatment for `RSS`, scaled against total system memory.

**Why.** Absolute byte figures give no sense of proportion. `2.1G` means
something quite different on an 8G box and a 512G one.

**Acceptance.** Scaled against `Sample::mem.total` from the *displayed* sample,
not the live one, so it stays correct while scrubbed.

**Depends on.** `P1`

**Touches.** `src/ui.rs`

---

## P3 — Per-process sparkline  ·  `M`

**What.** A small sparkline per row showing that process's CPU over the retained
window, drawn from history rather than from the current sample.

**Why this is the one to build.** Every retained `Sample` carries its complete
`Vec<ProcSample>`, so ptop can already answer "what has *this process* been
doing for the last ten minutes" without collecting anything new. No other
monitor can:

- htop, btop and bottom keep no per-process history at all.
- zenith's history is aggregate-only — `HistogramKind` has no per-process
  variant, and its process table evicts dead pids each tick.
- atop has the data but replays whole intervals from a logfile; it does not put
  a per-process trend beside the row in a live view.

It also converts the ring buffer from a feature you must go looking for into one
that is visible on every row — which is the difference between a differentiator
and a hidden mode.

**Design notes.**
- Reuse `History::peak_slots` for aggregation. Peak, never mean.
- Match a process across samples by pid **and** a stability check; pid reuse
  would otherwise splice two unrelated processes into one line. `ProcSample`
  has no start time yet — either add one (`/proc/<pid>/stat` field 22, which is
  already parsed past) or accept and document the risk.
- Cost is per visible row, not per process, so it scales with the viewport.

**Acceptance.** Sparkline covers the same window as the timeline and respects
zoom. Correct while scrubbed — it must show history *up to the cursor*, not up
to now, or it silently contradicts the table beside it. A pid reused inside the
window does not produce a spliced line. Render cost stays off the sample path
(`--bench` unchanged).

**Depends on.** `P1`

**Touches.** `src/ui.rs`, `src/history.rs`, possibly `src/sample.rs`
(process start time)
