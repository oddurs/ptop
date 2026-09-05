# Data fidelity

These are not UI items. They bound what ptop is *able* to answer, and `D1` is
the largest real gap against atop.

---

## D1 — Capture short-lived processes  ·  `L`

**What.** Account for processes that begin and end between two samples, so they
appear in the table for the interval they lived in.

**Why.** ptop samples once a second and reads `/proc` at that instant. A process
that lives 200ms is invisible — it never existed as far as ptop is concerned.
That is not an edge case for this tool: a burst of short-lived processes is one
of the most common causes of exactly the spike ptop exists to help you find, and
scrubbing back to the spike currently shows a process table that cannot explain
it.

atop solves this and is explicit about it — it reports "resource consumption by
all processes that were active during the interval, so also the resource
consumption by those processes that have finished during the interval". This is
the substantive capability difference between the two tools.

**Approaches, in preference order.**
1. **taskstats over netlink** — the kernel emits an exit record per process.
   Accurate, needs `CAP_NET_ADMIN`, Linux-only.
2. **BSD process accounting** (`acct(2)`) — needs root and a writable accounting
   file; more intrusive.
3. **Higher sample rate** — narrows the window without closing it. Cheap, and a
   reasonable interim step, but not a fix.

**Degrade honestly.** Without the required capability, say so in the UI rather
than showing an interval that quietly omits what happened in it. Same principle
already applied to gated IO: never a fabricated zero.

**Acceptance.** A process living 200ms appears in the interval that contains it,
marked as exited. Unavailable capability is disclosed, not silent. macOS path
degrades explicitly. Sampling cost measured before and after via `--bench`.

**Touches.** `src/collect/linux.rs`, `src/sample.rs`, `src/ui.rs`

---

## D2 — Mark sampling gaps  ·  `S`

**What.** Record the actual interval on each sample and render a visible break
in the timeline where it substantially exceeds the nominal one.

**Why.** A laptop that sleeps, or a box under heavy load, produces samples
minutes apart. The timeline currently renders those adjacent cells as if they
were one second apart, which silently compresses time and misleads. htop carries
a comment about this exact hazard — "period might be 0 after system sleep".
`History::time_behind` already reads real timestamps rather than counting rows;
this extends the same honesty to the graph.

**Acceptance.** A gap is visible and distinguishable from idle. Survives zoom
aggregation — a gap must not be averaged away.

**Touches.** `src/sample.rs`, `src/history.rs`, `src/ui.rs`

---

## D3 — Persist history across restarts  ·  `L`

**What.** Optionally write samples to disk and reload on start.

**Why.** Closes the remaining atop gap and zenith parity ("performance data
saved between runs"). It also changes what ptop *is*, from a live tool with a
memory to a small recorder — so it needs its own plan covering retention,
on-disk format, file size, and whether it implies a daemon. The zero-setup
pitch in `00-positioning.md` depends on ptop **not** requiring one, so this must
stay opt-in.

**Acceptance.** Off by default. Bounded on-disk size. Format versioned. Startup
cost with a full store is measured. Zero-setup behaviour unchanged when off.

**Touches.** new module, `src/history.rs`, `src/main.rs`

---

## D4 — Configurable sample interval  ·  `S`

**What.** `--interval=<secs>`, with the retained window expressed in time rather
than a fixed sample count.

**Why.** `HISTORY_LEN` is a hardcoded 600 samples and the interval a hardcoded
1s. Sub-second sampling narrows the `D1` window; longer intervals extend the
retained window on a quiet box. Both are useful, and the ring buffer already
tolerates uneven intervals because timestamps are real.

**Acceptance.** Window and interval independently configurable. Memory
implications documented. Titles and lag figures report real time, not sample
counts — they already do.

**Touches.** `src/main.rs`, `src/app.rs`, `src/ui.rs`
