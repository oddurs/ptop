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
| `Space` | pause on the current sample, or resume live |
| `Home` / `End` | jump to oldest / live |
| `↑` / `↓` | select a process |
| `s` | cycle sort column |
| `/` | filter by name or pid |

`ptop --once` prints a single plain-text sample and exits, for scripts and cron.

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
