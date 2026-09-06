# ptop

A system monitor you can rewind, with nothing to set up first.

ptop keeps every sample it takes — including the full process table — so you can
scrub backwards and ask what was eating the box forty seconds ago. It starts
with an empty buffer and fills it as it runs: no daemon, no config, no logfiles,
nothing that had to be running before you noticed the problem.

```
 ptop — PAUSED  -18s · warn 50 · crit 80
CPU  89.2%   MEM  37.5% (6.0G / 16.0G, 8.0G avail)   SWP  50.0%   LOAD 1.00 2.
── cores (4) ─────────────────────────────────────────────────────────────────
▇ ▄ ▁ █
── timeline — 5m00s of 10m00s buffered ───────────────────────────────────────
100 ⠤⣴⠤⢰⡄⢠⡆⠀⣦⠀⣴⠀⢰⡄⢠⡆⠤⣦⠤⣴⠤⢰⡄⢠⡆⠀⣦⠀⣴⠀⢰⡄⢠⡆⠤⣆⠤⣴⠤⣰⡀⢠⡆⢀⣆⠀⣴⠀⣰⡀⢰⡆⢀⣆⠤⣶⠤⣰⡀⢰⡆⢀⣆⠀⣶⠀⣰⡀⢰⡆⢀⣆⠤⣶
CPU ⡀⣿⡇⣾⣇⢸⣿⢰⣿⡀⣿⡇⣾⣇⢸⣷⢰⣿⠤⣿⡆⣾⡇⣸⣷⢸⣿⢀⣿⡆⣿⡇⣸⣷⢸⣿⢀⣿⡆⣿⡇⣸⣧⢸⣿⢀⣿⡄⣿⡇⣸⣧⢸⣿⢀⣿⡄⣿⡇⣼⣧⢸⣿⢠⣿⡄⣿⡇⣼⣧⢸⣿⢠⣿
  0 ⣷⣿⣇⣿⣿⣾⣿⣸⣿⣿⣿⣇⣿⣿⣿⣿⣸⣿⣿⣿⣇⣿⣿⣿⣿⣸⣿⣿⣿⣇⣿⣷⣿⣿⣸⣿⣾⣿⣇⣿⣷⣿⣿⣸⣿⣾⣿⣇⣿⣷⣿⣿⣸⣿⣼⣿⣧⣿⣧⣿⣿⣼⣿⣼⣿⣧⣿⣧⣿⣿⣼⣿⣼⣿
100 ⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀⠤⠀
MEM ⠤⣀⣀⣴⣾⣿⣿⣿⣿⣶⣄⣀⡀⠀⠤⠀⢀⣀⣠⣴⣾⣿⣿⣿⣷⣦⣄⣀⠤⠀⠤⠀⢀⣀⣤⣶⣿⣿⣿⣿⣶⣤⣀⣀⠤⠀⠤⠀⣀⣀⣤⣾⣿⣿⣿⣿⣶⣤⣀⡀⠤⠀⠤⠀⣀⣠⣴⣾⣿⣿⣿⣷⣦⣄
  0 ⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
                                               CPU 89.2%  MEM 37.5% ▐
2m28s shown, 1s/slot — ←/→ scrub, +/- zoom
── processes (4) — sort: CPU ─────────────────────────────────────────────────
PID     USER       CPU%   RSS      S  THR  COMMAND
824     root       88.4   512.0M   S  1    postgres
1190    root       12.5   32.0M    S  1    nginx
2077    root       4.2    148.0M   S  1    node
1       root       0.1    12.0M    S  1    systemd
q quit · ←/→ scrub · +/- zoom · Space live · ↑/↓ select · s sort · t tree · i
```

The gutter names each graph and anchors its scale; the dashed lines are the
warn and critical thresholds, drawn so the boundary is readable without relying
on colour. Bars absorb the rule where they cross it. The `▐` marks which sample
the cursor is on, down to which half of a braille cell.

The process table below the timeline is the real one from the moment under the
cursor, not an interpolation. Sampling continues while you are scrubbing.

## Prior art, and where ptop actually differs

ptop is not the first tool to let you look backwards, and it is not the most
capable one.

**[atop](https://www.atoptool.nl/)** has recorded historical per-process data
for years. It writes compressed daily logfiles, keeps 28 days by default, and
does one thing ptop cannot: it captures processes that started *and finished*
between two samples. If a burst of short-lived processes spiked your machine,
atop can show you and ptop currently cannot — see
[`docs/roadmaps/05-data-fidelity.md`](docs/roadmaps/05-data-fidelity.md).
On raw capability atop is the better tool.

**[zenith](https://github.com/bvaisvil/zenith)** has zoomable scroll-back charts
and saves data between runs. Its scrollback is aggregate-only, though: its
history holds CPU, memory, network, disk and GPU series, and its process table
renders from a live map that drops pids as they exit, so scrolling back moves
the charts but not the table.

**htop, btop and bottom** keep no history at all. They render the current
instant.

What ptop offers is narrower than "nobody else does this", and it is a usability
claim rather than a capability one:

- **Nothing has to have been running.** atop can only replay what its daemon
  already recorded. The common case — you connect to a machine that is slow
  *now* — is the case where that daemon was not running. ptop gives you the last
  ten minutes from a cold start.
- **One view, live and historical.** Scrubbing happens inside the running
  monitor, not in a separate replay mode against a logfile.
- **The process table follows the cursor.** Scrub to a spike and the table below
  it is the one from that instant.

If you are running a fleet and want history you can rely on after the fact,
install atop. If you want to know what this box is doing right now and what it
was doing a few minutes ago, that is what ptop is for.

|                                        | ptop | htop | btop | bottom | zenith | atop |
| -------------------------------------- | :--: | :--: | :--: | :----: | :----: | :--: |
| Live view                              |  ●   |  ●   |  ●   |   ●    |   ●    |  ●   |
| Rolling graph of recent values         |  ●   |  ◐¹  |  ●   |   ●    |   ●    |  ○   |
| Move backwards through time            |  ●   |  ○   |  ○   |   ◐²   |   ●    |  ●³  |
| Process table follows the time cursor  |  ●   |  ○   |  ○   |   ○    |   ○⁴   |  ●   |
| Captures processes that exited between samples | ○ | ○ | ○ |   ○    |   ○    |  ●   |
| History survives a restart             |  ○   |  ○   |  ○   |   ○    |   ●    |  ●   |
| Needs something running beforehand     |  ○   |  ○   |  ○   |   ○    |   ○    |  ●⁵  |

● yes · ◐ partial · ○ no

1. htop's Graph meter mode (`GRAPH_METERMODE`, `Meter.c`) keeps a rolling
   scalar buffer sized to the meter width. It is a graph, not navigable
   history, and it is per-meter rather than per-process.
2. bottom can freeze the display (`f`), but freezing gates the update
   (`if !app.data_store.is_frozen()`, `lib.rs`) rather than letting you look
   backwards. ptop keeps sampling while you scrub.
3. atop steps through intervals when replaying a logfile (`atop -r`), which is
   a separate mode rather than the live view.
4. zenith's `HistogramKind` holds only aggregate series; its process table
   renders from a live map that runs
   `.retain(|&k, _| current_pids.contains(&k))`.
5. atop's history requires its daemon to have been recording in advance. This
   row is the whole of ptop's argument.

Every cell above was checked against the tool's source or official
documentation rather than from memory; the footnotes name where.

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

`--color=auto|mono|16|256|true` picks the colour tier. Detected from
`COLORTERM` and `TERM`, and `NO_COLOR` is honoured. Each tier stands on its own:
monochrome is not a fallback but the proof that no meaning here is carried by
colour alone — thresholds, severity, selection and the paused state all survive
without it.

`--theme=safe|classic|auto` picks the palette. **`safe` is the default and
replaces green with cyan.** Green-and-yellow is the worst available pair for
red-green colour vision deficiency, which affects roughly 8% of men, and every
system monitor ships it: measured in OKLab ΔE×100 under Machado 2009 simulation
against a dark surface, ptop's old green↔yellow separated by **3.7** under
protanopia, against a target of 8. The shipped palette's worst pair among its
five meaning-bearing hues is **10.3**. `--theme=classic` restores
green/yellow/red for anyone who wants the convention back.

Both figures are **enforced in CI**, not asserted in prose: `src/cvd.rs`
implements the same Machado 2009 simulation and OKLab conversion the palette was
measured with, and a test fails the build if any pair among the five
meaning-bearing hues drops below ΔE 8. A second test enforces the contrast
floor, because separation between hues says nothing about whether a hue is
visible at all.

Both have already earned their place. The first caught the 256-colour tier
shipping `series_cpu` and `series_mem` only 6.7 apart, because rounding each
channel to its nearest cube level independently is not perceptually safe. The
replacement then failed the second, sitting at 2.03:1 against the selected-row
background — separated, and invisible.

`--glyphs=braille|block|ascii` picks how the timeline is drawn. Braille packs
two samples into every character cell and stacks cells vertically for twelve
distinct heights; `block` needs less font support; `ascii` needs none. A Linux
console (`TERM=linux`) selects `ascii` automatically.

Zooming aggregates samples into slots by **peak, never mean** — averaging a
100% spike with three idle samples would render 25% and hide the exact event
the tool exists to catch. Zoom is clamped to what the buffer can fill, so
zooming out never shrinks the graph into a corner; the empty region on the left
at full zoom is real time from before the buffer starts.

A laptop that sleeps, or a box loaded enough to miss its tick, leaves samples
minutes apart. Drawn as adjacent cells those claim to be one second apart, and
the x-axis quietly stops meaning anything. ptop draws a `┊` seam wherever at least
one interval went unobserved, full height and in chrome so it cannot be read as
a bar:

```text
CPU ⠀⠀⠀⠀⠀⠀⠀⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤┊⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤
  0 ⠀⠀⠀⠀⠀⠀⠀⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿┊⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿
1m20s shown, 1s/slot, ┊ time missing — ←/→ scrub, +/- zoom
```

A gap is *a missing sample*, not a slow one, so the line falls at twice the
nominal interval and needs no tuning. Sampling runs on a fixed cadence rather
than one interval after the previous sample finished — otherwise collection and
draw time are added to every period, the timestamps drift away from the rate
they claim, and on a busy box the detector would eventually see a missed tick
on every cell. If the host genuinely cannot sustain the interval, the samples
really are more than one tick apart and the seams are telling you so. Gaps aggregate by **or** for
the same reason values aggregate by peak — zooming out must not be able to
erase an event, least of all at the zoom where the whole buffer is on screen.

## Configuration

`~/.config/ptop/ptop.conf`, honouring `$XDG_CONFIG_HOME`. Every flag is a
`key = value` line without the leading dashes:

```ini
# ~/.config/ptop/ptop.conf
theme  = classic
glyphs = block      # comments run to the end of the line
color  = 256
```

Precedence, lowest first: built-in default, config file, `NO_COLOR`, flag. The
flag always wins, so a wrapper script can override a user's file without
editing it; `NO_COLOR` outranks the file because the file records a preference
in general and the environment is saying something about this terminal now.

**An unknown key warns and ptop starts anyway**, naming the key and the line —
and guessing what you meant:

```
ptop: ~/.config/ptop/ptop.conf:6: unknown key `colour` (did you mean `color`?)
```

A bad line in the file warns; a bad flag is fatal. The asymmetry is deliberate:
a config file is written once and read every run, so one typo must not cost you
the tool, but a flag was typed for *this* run and quietly ignoring it would do
something other than what was asked.

Hand-rolled `key = value`, not TOML. ptop's config surface is genuinely flat,
and `serde` + `toml` would be the largest dependency in the project by an order
of magnitude — in a codebase whose `/proc` parser is deliberately hand-rolled
with no dependencies at all. htop and btop both use `key = value` and neither
has outgrown it.

One table defines every setting once, and both the file and the command line
drive it, so `theme = classic` and `--theme=classic` cannot come to disagree
about what a value means.

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

Open work is tracked in-repo with [cairn](https://github.com/oddurs/cairn) —
see [ROADMAP.md](ROADMAP.md), or `cairn board` in a checkout.

The reasoning behind each decision lives in [`docs/roadmaps/`](docs/roadmaps/),
derived from reading the prior art (htop, btop, bottom, zenith, atop) and
auditing this UI against data-visualisation practice. Start with
[the index](docs/roadmaps/README.md).

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
