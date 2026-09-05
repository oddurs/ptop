# Collection efficiency

ptop is not slow today, and that is the wrong reason to leave the collector
alone. At 1 Hz and 400 processes a sample costs 1.44 ms — 0.14% of one core.
But the cost is linear in both sample rate and process count, and two items
already on the roadmap push on both:

| | 1 Hz | 10 Hz |
| --- | --- | --- |
| 400 procs | 0.14% | 1.44% |
| 4000 procs | 1.44% | **14.4%** |

`D1` wants a faster sample rate to narrow the window in which a short-lived
process is invisible. `D4` exposes that rate to the user. A monitor burning 14%
of a core to watch a busy box is part of the problem it is meant to diagnose, so
these two land **before** the D series rather than after it.

## The measurement

`strace -c` on `--bench`, 304 processes, per sample:

| syscall | calls | per process |
| --- | --- | --- |
| `read` | 2574 | 8.5 |
| `statx` | 802 | 2.6 |
| `openat` | 484 | 1.6 |
| `close` | 484 | 1.6 |

Retained footprint at 400 processes × 600 samples: **26.9 MB** — 23 MB of
structs plus 3.8 MB of process names, with `ProcSample` at 96 bytes.

---

## E1 — Read `/proc` into a reused buffer  ·  `M`

**What.** Replace `fs::read_to_string(format!("/proc/{pid}/stat"))` with an
`openat` relative to a cached directory fd, one `read` into a buffer owned by
the collector, and a `close`.

**Why.** Each `read_to_string` allocates a path `String`, then does
`openat` → `statx` → several `read`s → `close`. The `statx` exists to size a
buffer, and `/proc` files report size 0, so it is pure waste — and because the
size is unknown the buffer grows, which is where 8.5 reads per process comes
from.

htop does the same job with `openat` on a cached dirfd and **one** `read` into a
fixed stack buffer (`Compat_readfileat` → `readfd_internal`). btop is
equivalent. Neither calls `stat` to size anything.

**Acceptance.** `statx` calls per sample fall to roughly one per process (the
uid `fstat`, which is genuinely needed). `read` calls approach one per file
opened. `--bench` improves and the number is recorded here. No behaviour change:
the existing `/proc` parsing tests are untouched and still pass.

**Touches.** `src/collect/linux.rs`

---

## E2 — Intern process names  ·  `S`

**What.** Hold `ProcSample::name` as `Arc<str>`, cached per pid and refreshed
only when the pid is new to the collector.

**Why.** The name is re-parsed and re-allocated for every process on every
sample. At 400 processes over a 600-sample buffer that is **240,000 `String`
allocations retained, 3.8 MB**, for strings that essentially never change.

`user` is already interned this way — it was the fix that made the uid lookup
2.26× faster — and `name` never got the same treatment. htop reads `comm` and
`cmdline` **once per process lifetime**, gated on `!preExisting`; btop gates the
same reads on `no_cache`. ptop has no such gate.

**The wrinkle.** A pid can be reused, and the new process will usually have a
different name. The cache must be keyed on something that distinguishes them, or
a recycled pid inherits the old process's name. `/proc/<pid>/stat` field 22 is
the start time and is already parsed past; pairing it with the pid gives a key
that survives reuse. Without it the cache is a correctness bug, not an
optimisation.

**Acceptance.** Allocations per sample drop from one-per-process to one per
*new* process. Retained memory falls by roughly the 3.8 MB above. A pid reused
within the buffer window does not inherit the previous process's name — tested
directly, since that is the failure mode this introduces.

**Touches.** `src/sample.rs`, `src/collect/linux.rs`, `src/collect/darwin.rs`

---

## Deliberately not doing

Measured and left alone, because saying "this is fine" is worth as much as a
fix:

- **Render: 356 µs/frame** at 900 processes on a 200×60 terminal. Every glyph is
  a separately allocated `String` — about 2,400 per frame — and at 1 fps it does
  not matter.
- **`visible_rows()` is rebuilt up to three times per keypress** (`select_delta`,
  `clamp_selection`, `draw_procs`) at 15.6 µs each. Roughly 47 µs on a keypress.
- **`ProcSample` could be smaller** — `state: char` spends 4 bytes on one byte of
  information. Worth perhaps 1.5 MB of the 26.9 MB, and it would make the field
  less obvious at every use site.
