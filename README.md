# ptop

A system monitor you can rewind.

`htop`, `btop`, and `bottom` all show you *now*. You notice a CPU spike, tab over
to look, and it's already gone. ptop keeps every sample it takes — including the
full process table — so you can scrub backwards and ask what was actually eating
the box forty seconds ago.

```
┌ ptop — PAUSED  -18s ─────────────────────────────────────────────────────────┐
│CPU  50.9%   MEM  62.5% (10.0G / 16.0G, 8.0G avail)   LOAD 1.00 2.00 3.00     │
└──────────────────────────────────────────────────────────────────────────────┘
┌ timeline — 90s shown, 90s of 600s buffered ──────────────────────────────────┐
│▁▅█▇▃▃▇█▅▁▅█▇▃▃▇█▅▁▅█▇▃▄▇▇▅▁▆█▇▃▄▇▇▅▁▆█▇▃▄▇▇▅▂▆█▆▃▄▇▇▅▂▆█▆▃▄▇▇▅▂▆█▆▂▄▇▇▅▂▆█▆▂│
│▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▄▃▃▃▃▃▃▃▃▄▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▅▄▄▄▃▃▃▃▃▃▃▃▃▄▄▄▅▅▅▅▅▅▅▅▅▅▅▅▅▅│
│                                              ▲                               │
└──────────────────────────────────────────────────────────────────────────────┘
```

The process table below the timeline is the real one from the moment under the
cursor, not an interpolation. Sampling continues while you are scrubbing.

## Build

```sh
cargo build --release
./target/release/ptop
```

## Keys

| Key | Action |
| --- | --- |
| `q` | quit |
| `←` / `→` | scrub through history (hold `Shift` for ten at a time) |
| `+` / `-` | zoom the timeline in and out |
| `Space` | pause on the current sample, or resume live |
| `Home` / `End` | jump to oldest / live |
| `↑` / `↓` | select a process |
| `s` | cycle sort column |
| `t` | toggle the process tree |
| `i` | toggle per-process disk IO columns |
| `/` | filter by name or pid |

`ptop --once` prints a single plain-text sample and exits, for scripts and cron.
`ptop --bench` times 20 collection passes, for checking the cost of a change.

`--glyphs=braille|block|ascii` picks how the timeline is drawn. Braille packs
two samples into every character cell and stacks cells vertically for twelve
distinct heights; `block` needs less font support; `ascii` needs none. A Linux
console (`TERM=linux`) selects `ascii` automatically.

Zooming aggregates samples into slots by **peak, never mean** — averaging a
100% spike with three idle samples would render 25% and hide the exact event
the tool exists to catch. Zoom is clamped to what the buffer can fill, so
zooming out never shrinks the graph into a corner; the empty region on the left
at full zoom is real time from before the buffer starts.

## How it works

Two backends behind one `Collector` trait:

- **Linux** (`src/collect/linux.rs`) parses `/proc` directly with nothing but
  `std`. This is the interesting one. It handles the parts that catch people
  out: `/proc/[pid]/stat` field 2 can contain spaces *and* parentheses, so it
  splits on the last `)` rather than on whitespace; `guest` and `guest_nice` in
  `/proc/stat` are already counted inside `user` and `nice`, so summing every
  field double-counts them; and every CPU figure is a delta between two reads,
  which is why the first sample always reports zero.
- **macOS/other** (`src/collect/darwin.rs`) goes through `sysinfo`, so the tool
  runs on a dev laptop. There is no `/proc` here and the mach calls that replace
  it are a different project's worth of `unsafe`.

History lives in a fixed-capacity ring buffer (`src/history.rs`). "Live" is
represented as *absence* of a cursor rather than an index pinned to the end, so
there is exactly one representation of it and pushes never have to fix the
cursor up. When the window slides, a pinned cursor slides with it — otherwise it
would silently drift forward through time while appearing to stay put.

Ten minutes of scrollback at one sample per second, which is roughly 30 MB of
process tables on a busy machine.

## Notes from reading the others

Lessons taken from htop, btop, and bottom, with the measurements that backed
them:

- **Cache what doesn't change.** htop reads a process's cmdline and start time
  once per process *lifetime*, not once per sample, and gets the uid from a
  single `fstat` on the `/proc/<pid>` directory rather than parsing
  `/proc/<pid>/status`. ptop originally parsed that file for every process every
  second: at 400 processes that was 58% of total collection time. Switching to
  `fstat` took a sample from 3.19ms to 1.41ms.
- **Clamp CPU percentages.** htop does `MINIMUM(percent_cpu, activeCPUs * 100)`.
  Without it, a pid reused between two samples diffs the new process against the
  old one's counter and reports thousands of percent. ptop now clamps the same
  way.
- **Guard the sample interval.** htop carries the comment "period might be 0
  after system sleep" — a real bug someone hit on a laptop, worth knowing about
  before it happens to you.
- **Only collect what is displayed.** htop gates expensive reads behind
  `PROCESS_FLAG_*` bits derived from the visible columns, so turning off a
  column stops the syscalls behind it. ptop collects everything unconditionally;
  this is the right shape to adopt before adding per-process IO and network.
- **Spread expensive work across samples.** For costly `/proc/<pid>/maps`
  parsing htop re-checks each process on a randomised interval rather than doing
  every process on the same tick, which avoids a periodic stall.
- **Have a fallback glyph set.** btop keeps a `tty_mode` symbol table for
  terminals that cannot render braille. Any Unicode-dependent drawing needs a
  plain-ASCII path.

All three are implemented: braille rendering and timeline zoom
(`src/glyphs.rs`, `History::peak_slots`), the process tree (`src/tree.rs`), and
tiered collection (`collect::Needs`).

**Tiered collection.** Core figures — cpu, memory, rss, name, user, state,
threads — are never gated, because the timeline and the default table depend on
them and their history has to be complete. Per-process disk IO is gated on the
column being visible, and it is not cheap: at 400 processes a sample costs
1.55ms without it and 2.28ms with, since it adds a file read per process.

**Gated collection conflicts with rewindable history** in a way htop never has
to face — enable IO at t=300 and the first 300 samples have nothing to show when
you scrub back into them. ptop resolves this by making collection a *ratchet*:
showing the columns starts collection, hiding them does not stop it. Toggling
would otherwise punch holes wherever the column happened to be off, and one
clean boundary is far easier to reason about while scrubbing than several.

Three states are rendered, and **none of them is a zero** — a fabricated zero is
indistinguishable from a genuinely idle process:

| Shown | Meaning |
| --- | --- |
| `·` | history recorded before the column was switched on |
| `—` | unreadable, or the process is too new to have a second reading yet |
| `1.2M/s` | a real rate |

`/proc/<pid>/io` is mode `0400` and owned by the process owner, so reading other
users' processes needs `CAP_SYS_PTRACE`. Only genuinely unreadable processes
prompt for root; a process merely awaiting its second reading also shows a dash
but resolves on its own, and conflating the two produces advice that does not
help.

The tree is a view over `ppid`, which every retained sample already carries, so
the tree you see while scrubbed back is the real hierarchy from that moment.
Every process appears exactly once even when `ppid` is corrupt or cyclic, and a
process orphaned between samples becomes a root rather than disappearing.

**macOS caveat:** `sysinfo` reports no parent for processes the user does not
own, so about a third of pids arrive with `ppid = 0` and appear as roots. The
`/proc` backend has real parentage for everything.

## Roadmap

Granular, evidence-backed items live in [`docs/roadmaps/`](docs/roadmaps/),
derived from reading the prior art (htop, btop, bottom, zenith, atop) and
auditing this UI against data-visualisation practice. Start with
[the index](docs/roadmaps/README.md).

Note that [`docs/roadmaps/00-positioning.md`](docs/roadmaps/00-positioning.md)
records a correction: **atop already does historical per-process replay**, and
better than ptop in one important respect — it captures processes that exited
between samples, which ptop cannot yet see. What is left is a usability
position, not a capability one. The claims elsewhere in this README are being
revised accordingly.

## Status

Early. What works: both backends, the timeline and scrubbing, sorting,
filtering, `--once`. Not there yet: process tree view (`ppid` is collected but
unused), per-process disk and network attribution, killing processes,
configurable intervals, persisting history across restarts.

## Tests

```sh
cargo test                     # host backend
cargo test -- --ignored --nocapture show_frame   # print a rendered frame
docker run --rm -v "$PWD":/w -w /w -e CARGO_TARGET_DIR=/tmp/t rust:1-slim cargo test
```

The last one matters on a Mac: the `/proc` backend is `cfg`'d out of a macOS
build entirely, so it is neither compiled nor tested unless you run it on Linux.

UI tests render through ratatui's `TestBackend` and assert on the resulting
buffer, including a 1×1 terminal — a monitor that panics on a small window is
worse than no monitor, and it never shows up in normal use.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
