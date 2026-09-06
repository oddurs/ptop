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
theme    = classic
glyphs   = block    # comments run to the end of the line
color    = 256
warn     = 65       # a build box is busy at 50% and perfectly fine
critical = 90
interval = 500ms    # every sample keeps a whole process table,
window   = 30m      # so these two together decide the memory
```

`warn` and `critical` are where the status colours change, where the timeline
draws its threshold rules, and what the header legend prints. All three read
the same pair, so the printed numbers cannot drift from the behaviour they
describe — that agreement is the entire value of printing them.

One rule governs the pair: **every status band must be reachable.** `heat`
reads `[0, warn)` as ok, `[warn, critical)` as warn and `[critical, 100]` as
critical, so `warn` above zero and `critical` above `warn` is exactly the
condition for none of the three to be empty. That rejects an inverted pair, an
equal one (the warn band would be empty), and `warn = 0` (nothing would ever be
ok, and a rule would sit permanently along the bottom of both graphs).
`critical = 100` is allowed: its band is the single point 100, but a machine
really does reach 100% memory, so the band is reachable rather than empty.

Out-of-range values are rejected rather than clamped — `warn = 150` is someone
who has misunderstood the units, and quietly turning it into 100 hides that
from them for as long as they use the tool. Fractions are fine, and the header
prints them at the precision you gave: `warn = 62.5` shows `warn 62.5`, not a
rounded `62` claiming the colour changes half a point from where it does.

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

### Themes

The palette is compiled in, but it is not the only one you can have. A theme is
one file, one line per colour, in `~/.config/ptop/themes/NAME.theme`:

```ini
# ~/.config/ptop/themes/nord.theme
ok         = #8fbcbb    # hex,
series_cpu = 67         # a 256-colour index,
chrome     = darkgray   # or an ANSI name
```

**Every line is optional.** A theme inherits `safe` for anything it does not
name, so overriding two colours is two lines. That is the whole reason btop
ships 41 themes and htop, which hardcodes eight in C, has never gained a ninth:
the barrier decides whether a theme library exists.

The tokens are exactly the ten the palette was built around — `ok`, `warn`,
`critical`, `series_cpu`, `series_mem`, `chrome`, `text`, `text_dim`,
`selection_bg`, `live`. Nothing new is invented for user themes; a vocabulary
the code did not already use would be a second one to keep in step with the
first.

**The built-ins ship as files too**, in [`themes/`](themes/), so the way to
learn the format is to copy one — and a test asserts they match the compiled
palettes colour for colour, so they cannot quietly drift.

A built-in name always wins over a file. A `safe.theme` in your themes
directory would otherwise silently replace the palette everything else in this
project is measured against.

**A colour the terminal cannot show is left alone and reported, not
approximated.** Hex needs a true-colour terminal, an index needs 256 colours, a
name works anywhere, and monochrome ignores all of them:

```
ptop: theme `nord`: this terminal is Ansi16 and cannot show ok, series_cpu;
      keeping ok = cyan, series_cpu = lightblue
```

Squeezing 24-bit hex into sixteen slots would destroy exactly the separation
the palettes were measured for — and those sixteen slots belong to your
terminal theme, not to ptop.

### Measuring a theme

ptop is the only monitor that measures its own palette, and the moment you can
supply your own that guarantee evaporates — unless the validator is turned
outward. So it is:

```
$ ptop --check-theme muddy
muddy: FAIL
  ok          ↔ warn        ΔE   0.1  tritan    below the target of 8
  ok          ↔ critical    ΔE  54.2  deutan
  ...
  critical    on surface         1.09:1          below 3:1
  critical    on selected row    1.40:1          below 3:1
  worst pair: ΔE 0.1   worst contrast: 1.09:1
```

Every pair, not only the failures: a theme passing at ΔE 8.1 is a different
thing from one passing at 30, and the number is the point. It exits non-zero on
failure, so it works in a script — **a contributed theme can arrive with a
measurement rather than a screenshot.**

**This is the same instrument CI uses.** The palette tests assert through
`check::Report` rather than a second copy of the arithmetic, so ptop's own
check and yours cannot come to disagree about the same colours.

**A failing theme still loads**, with one line saying why:

```
ptop: theme `muddy`: ok and warn are only ΔE 0.1 apart (tritan), critical is
      1.09:1 on the surface — run `ptop --check-theme muddy` for the rest
```

It is your terminal and your choice; ptop's job is to have the number and say
it, not to refuse — the same principle as rendering `—` rather than a
fabricated zero.

`--check-theme classic` reports FAIL, and says why that is a decision rather
than a bug: classic exists to restore the green/yellow convention, and
green/yellow is the pair that convention gets wrong under red-green deficiency.
That is the whole argument for `safe` being the default, and it is pinned by a
test so that quietly "improving" classic would break the build rather than
remove the argument.

### Sample rate and window

`interval` is the time between samples; `window` is how much history to keep,
**expressed in time rather than a sample count** — the span is what you
actually want, and the buffer size follows from it. Spans are written the way
people write them: `500ms`, `2s`, `10m`, `1h`. A bare number is seconds,
because that is what someone typing `interval = 2` means.

Sub-second sampling narrows the window in which a short-lived process is
invisible; longer intervals stretch the retained span on a quiet box. The ring
buffer already tolerates uneven intervals, because the timestamps are real.

**The two settings cost memory together, not separately.** Every sample retains
its whole process table — that is what makes scrubbing show the real table from
that instant rather than an interpolation — so the buffer is about
`samples × processes × 96 bytes`:

| processes | 10m at 1s | 1h at 1s | 10m at 100ms |
|---|---|---|---|
| 100 | 6 MB | 35 MB | 59 MB |
| 400 | 23 MB | 139 MB | 231 MB |
| 4000 | 231 MB | 1.4 GB | 2.3 GB |

(a buffer holds one more sample than the span needs — `n` samples span `n − 1`
intervals — so the real figures are a few kilobytes above these)

(measured by the `show_sample_footprint` test, so the table can't drift from
the structs)

A day of history at one sample a second and ten minutes at sixty a second are
the same buffer, so ptop bounds the **product** — the sample count — rather
than either setting, and says what the buffer would cost when it refuses:

```
ptop: `window` 86400s at `interval` 500ms is 172801 samples, above the limit
      of 86401; every sample retains a whole process table, which is about
      6663 MB on a 400-process box
```

The interval is also bounded below at 50ms: a collection pass costs about 1 ms
at 400 processes, so 50 ms already spends 2% of a core and 10 ms would spend
10%. A monitor that is itself the load is not measuring the machine.

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
