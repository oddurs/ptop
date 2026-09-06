//! ptop — a system monitor you can rewind.
//!
//! Copyright (C) 2026  ptop contributors
//!
//! This program is free software: you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation, either version 3 of the License, or (at your option)
//! any later version. See the LICENSE file for details.

mod app;
mod check;
mod collect;
mod config;
mod cvd;
mod glyphs;
mod history;
mod sample;
mod theme;
mod tree;
mod ui;

#[cfg(test)]
mod ui_tests;

use app::{App, Sort};
use collect::{Collector, Needs, Platform};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use std::io;
use std::time::{Duration, Instant};

const USAGE: &str = "\
ptop — a system monitor you can rewind

USAGE:
    ptop            interactive mode
    ptop --once     print one plain-text sample and exit
    ptop --bench    time 20 collection passes (development)
    ptop --check-theme NAME
                    measure a theme and say whether it is legible

    --glyphs=SET    timeline drawing: braille (default), block, or ascii.
                    Falls back to ascii automatically on a Linux console.
    --color=TIER    auto (default), mono, 16, 256, or true. Honours NO_COLOR.
    --interval=SPAN time between samples: 500ms, 2s, 10m (default 1s)
    --window=SPAN   history retained, as time not samples (default 10m)
    --warn=PCT      where 'getting busy' begins (default 50)
    --critical=PCT  where 'in trouble' begins (default 80). Must exceed --warn.
    --theme=NAME    a built-in (safe, classic, auto) or a file in
                    ~/.config/ptop/themes/NAME.theme. 'safe' replaces green with
                    cyan: green/yellow separates by only dE 3.7 under simulated
                    protanopia, against a target of 8, and red-green deficiency
                    affects roughly 8% of men. 'classic' restores green/yellow/red.

CONFIG:
    ~/.config/ptop/ptop.conf, honouring $XDG_CONFIG_HOME. Every setting above
    is a `key = value` line without the leading dashes:

        theme    = classic
        glyphs   = block    # comments run to the end of the line
        color    = 256
        warn     = 65       # a build box is busy at 50% and fine
        critical = 90
        interval = 500ms    # every sample keeps a whole process table,
        window   = 30m      # so these two together decide the memory

    Lowest precedence first: built-in default, config file, NO_COLOR, flag —
    so a wrapper script can override a user's file without editing it.

    An unknown key warns, naming the key and the line, and ptop starts anyway.
    One typo should not cost you the tool.

THEMES:
    ~/.config/ptop/themes/NAME.theme, one line per colour. Every line is
    optional — a theme inherits `safe` for anything it does not name:

        ok         = #8fbcbb    # hex,
        series_cpu = 67         # a 256-colour index,
        chrome     = darkgray   # or an ANSI name

    Tokens: ok, warn, critical, series_cpu, series_mem, chrome, text,
    text_dim, selection_bg, live. The built-ins ship as files too, so the way
    to learn the format is to copy one.

    `ptop --check-theme NAME` measures one: the separation between every pair
    of meaning-bearing hues under simulated colour vision deficiency, and the
    contrast of each against the backgrounds it is drawn over. It exits
    non-zero on failure, so it works in a script. A failing theme still loads,
    with one line saying why — it is your terminal and your choice.

OPTIONS:
    -h, --help      show this help
    -V, --version   show version

KEYS:
    q               quit
    Left/Right      scrub through history (Shift for 10 at a time)
    + / -           zoom the timeline in and out
    Space           pause on the current sample, or resume live
    Home/End        jump to oldest / live
    Up/Down         select a process
    s               cycle sort column
    t               toggle the process tree
    i               toggle per-process disk IO columns
    /               filter by name or pid
";

/// Print a line, stopping the program quietly if the reader has gone away.
///
/// Rust ignores SIGPIPE and turns the resulting write error into a panic, so
/// `ptop --once | head -1` died with a backtrace. `--once` exists to be
/// scriptable, and `| head`, `| grep -m1` and `| less` are how a scriptable
/// thing gets used — a monitor that panics when you page its output is not one.
///
/// Handled in the writer rather than by restoring the signal disposition,
/// which would need `libc` for two lines of behaviour.
macro_rules! outln {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        if writeln!(std::io::stdout(), $($arg)*).is_err() {
            return Ok(());
        }
    }};
}

/// Print what could not be used.
///
/// Held rather than printed where it was found, because config is read before
/// the alternate screen opens and the screen erases everything written before
/// it. A warning nobody can see is not a warning, so the interactive path
/// waits until the terminal is its own again.
fn flush(warnings: &[config::Warning]) {
    for w in warnings {
        eprintln!("ptop: {w}");
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut collector = Platform::new()?;

    let mut warnings = Vec::new();
    let file = config::read(&mut warnings);
    let no_color = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let (settings, positional, file_warnings) = config::resolve(
        config::Settings::detect(),
        config::Sources {
            file: file.as_ref().map(|(o, t)| (o.as_str(), t.as_str())),
            no_color,
            themes: &config::read_theme,
        },
        &args,
    )
    .unwrap_or_else(|bad| {
        // Before exiting, not after: a config file that could not be *read* is
        // reported here too, and dropping that warning because a flag was also
        // wrong would send the user off to fix the flag and rerun into the
        // same silently-ignored config.
        flush(&warnings);
        eprintln!("ptop: {}", bad.as_flag());
        std::process::exit(2);
    });
    warnings.extend(file_warnings);

    let args = positional;

    // Built here rather than beside the App, so that a problem with the user's
    // theme is reported on every path — a colour scheme nobody can read is a
    // fact about their config, and `--once` and `--version` report every other
    // config problem too.
    let (theme, skipped) = theme::Theme::new(settings.palette, settings.tier)
        .with_thresholds(settings.warn, settings.critical)
        .with_overrides(&settings.overrides);
    // A failing theme still loads, with one line saying so. It is the user's
    // terminal and their choice; ptop's job is to have the number and say it,
    // not to refuse — the same principle as rendering `—` rather than a
    // fabricated zero. Only for user themes: a built-in's shortfall is a
    // decision already made and documented, not news.
    if !settings.overrides.is_empty()
        && let Some(warning) = check::Report::of(&settings.theme, &theme).warning()
    {
        warnings.push(config::Warning(warning));
    }
    // Said once rather than per colour: a 256-colour terminal reading a
    // true-colour theme would otherwise print ten near-identical lines, and
    // the useful fact is which terminal you are on, not which token was first.
    if let Some(note) = config::theme_note(
        settings.tier,
        &settings.theme,
        &theme,
        &settings.overrides,
        &skipped,
    ) {
        warnings.push(config::Warning(note));
    }

    match args.first().map(String::as_str) {
        Some("--once") => {
            flush(&warnings);
            return once(&mut collector, settings.interval);
        }
        Some("--bench") => {
            flush(&warnings);
            // Measure with extended collection both off and on, so the cost
            // of gating a column is a number rather than a claim.
            let n = 20;
            for needs in [Needs { io: false }, Needs { io: true }] {
                collector.sample(needs)?;
                let t0 = std::time::Instant::now();
                let mut count = 0;
                for _ in 0..n {
                    count = collector.sample(needs)?.procs.len();
                }
                let label = if needs.io { "io on " } else { "io off" };
                outln!("{label}: {count} procs, {:?}/sample", t0.elapsed() / n);
            }
            return Ok(());
        }
        Some("--help" | "-h") => {
            flush(&warnings);
            outln!("{USAGE}");
            return Ok(());
        }
        Some("--check-theme") => {
            flush(&warnings);
            let Some(name) = args.get(1) else {
                eprintln!("ptop: --check-theme needs a theme name");
                std::process::exit(2);
            };
            return check_theme(name, settings.tier);
        }
        Some("--version" | "-V") => {
            flush(&warnings);
            outln!("ptop {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(other) => {
            flush(&warnings);
            eprintln!("ptop: unrecognised option '{other}'\n\n{USAGE}");
            std::process::exit(2);
        }
        None => {}
    }

    let mut app = App::new(settings.history_len());
    app.interval = settings.interval;
    app.theme = theme;
    app.glyphs = settings.glyphs;

    // Collect once before drawing so the first frame has real numbers. CPU
    // still reads zero — there is no previous counter to diff against yet.
    app.push(collector.sample(app.needs())?);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &mut collector);
    ratatui::restore();
    flush(&warnings);
    result
}

/// Measure a theme and say whether it is legible, for scripts and reviewers.
///
/// The side effect worth having: a contributed theme arrives with a
/// measurement rather than a screenshot.
///
/// Exits non-zero on failure, so it is usable in a pipeline. The tier is the
/// detected one, because a theme is only legible on a terminal that can show
/// it — checking hex against a 16-colour terminal would report on colours that
/// will never appear.
fn check_theme(name: &str, tier: theme::Tier) -> io::Result<()> {
    let mut warnings = Vec::new();
    let (palette, overrides) =
        match config::resolve_named_theme(name, &config::read_theme, &mut warnings) {
            Ok(pair) => pair,
            Err(why) => {
                eprintln!("ptop: {why}");
                std::process::exit(2);
            }
        };
    flush(&warnings);
    let (built, skipped) = theme::Theme::new(palette, tier).with_overrides(&overrides);
    if !skipped.is_empty() {
        eprintln!(
            "ptop: {} of this theme's colours need a better terminal than this {tier:?} one, \
             and are measured as the built-in values they fall back to",
            skipped.len()
        );
    }
    // A user theme inherits `safe`, which has no caveat; a built-in speaks
    // for itself.
    let report = check::Report::of(name, &built)
        .with_caveat(theme::Palette::parse(name).and_then(theme::Palette::caveat));
    outln!("{report}");
    if !report.passes() {
        std::process::exit(1);
    }
    Ok(())
}

/// Print one sample as plain text and exit.
///
/// Two samples are taken, one interval apart: CPU figures are deltas between
/// reads, so a single sample could only ever report zero.
fn once(collector: &mut impl Collector, interval: Duration) -> io::Result<()> {
    let needs = Needs { io: true };
    collector.sample(needs)?;
    std::thread::sleep(interval);
    let s = collector.sample(needs)?;

    outln!(
        "cpu    {:.1}%  ({} cores)",
        s.cpu_total,
        s.cpu_per_core.len()
    );
    outln!(
        "mem    {:.1}%  {} / {} used, {} available",
        s.mem.used_pct(),
        human(s.mem.used),
        human(s.mem.total),
        human(s.mem.available)
    );
    if s.mem.swap_total > 0 {
        outln!(
            "swap   {:.1}%  {} / {}",
            s.mem.swap_pct(),
            human(s.mem.swap_used),
            human(s.mem.swap_total)
        );
    }
    outln!("load   {:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2]);
    outln!("procs  {}", s.procs.len());
    if s.io_denied > 0 {
        outln!(
            "io     {}/{} processes unreadable — run as root to see them",
            s.io_denied,
            s.procs.len()
        );
    }

    let mut top = s.procs.clone();
    top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    outln!(
        "\n{:>7}  {:>6}  {:>9}  {:>10}  {:>10}  COMMAND",
        "PID",
        "CPU%",
        "RSS",
        "DISK R/s",
        "DISK W/s"
    );
    for p in top.iter().take(10) {
        // A dash, never a zero: this process could not be read, which is not
        // the same as it doing no IO.
        let (r, w) = match p.io {
            Some(io) => (human(io.read), human(io.write)),
            None => ("—".into(), "—".into()),
        };
        outln!(
            "{:>7}  {:>6.1}  {:>9}  {r:>10}  {w:>10}  {}",
            p.pid,
            p.cpu,
            human(p.rss),
            p.name
        );
    }
    Ok(())
}

fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "K", "M", "G", "T"];
    let (mut v, mut i) = (b as f64, 0);
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1}{}", U[i])
}

fn run(
    terminal: &mut ratatui::DefaultTerminal,
    app: &mut App,
    collector: &mut impl Collector,
) -> io::Result<()> {
    let interval = app.interval;
    // A fixed cadence, not "one interval after the last sample finished".
    //
    // Restarting the clock after collection adds the collect and draw time to
    // every period, so the timestamps drift steadily away from the rate they
    // claim — on a box with thousands of processes, far enough that the gap
    // detector would see a missed tick on every single cell and paint the
    // whole graph as seams. The interval is a schedule, so schedule against it.
    let mut next_sample = Instant::now() + interval;

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll with whatever is left of the sample interval: input stays
        // responsive without spinning, and sampling stays on schedule.
        let timeout = next_sample.saturating_duration_since(Instant::now());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key.code, key.modifiers);
        }

        if Instant::now() >= next_sample {
            // Sampling continues while paused — that is the whole point. The
            // cursor stays put, the buffer keeps filling behind it.
            app.push(collector.sample(app.needs())?);
            app.clamp_selection();
            next_sample += interval;
            // Falling a whole interval behind means the host cannot sustain
            // the rate. Resync rather than catch up: catching up would sample
            // flat out until the backlog cleared, which is the worst thing to
            // do to the loaded box that caused the backlog. The samples really
            // are further apart than the nominal rate, and the timeline says
            // so — that is what the seam is for.
            let now = Instant::now();
            if next_sample <= now {
                next_sample = now + interval;
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, code: KeyCode, mods: KeyModifiers) {
    if app.editing_filter {
        match code {
            KeyCode::Enter | KeyCode::Esc => app.editing_filter = false,
            KeyCode::Backspace => {
                app.filter.pop();
                app.clamp_selection();
            }
            KeyCode::Char(c) => {
                app.filter.push(c);
                app.clamp_selection();
            }
            _ => {}
        }
        return;
    }

    match code {
        KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
        KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => app.should_quit = true,

        // Scrubbing. Shift jumps ten samples at a time for crossing a long
        // buffer without holding the key down.
        KeyCode::Left | KeyCode::Char('h') => {
            let step = if mods.contains(KeyModifiers::SHIFT) {
                10
            } else {
                1
            };
            app.history.scrub(-step);
            app.clamp_selection();
        }
        KeyCode::Right | KeyCode::Char('l') => {
            let step = if mods.contains(KeyModifiers::SHIFT) {
                10
            } else {
                1
            };
            app.history.scrub(step);
            app.clamp_selection();
        }
        KeyCode::Char(' ') => {
            // Space toggles: pause pins the cursor where it is, resume returns
            // to the live edge.
            if app.history.is_live() {
                app.history.scrub(-1);
            } else {
                app.history.goto_live();
            }
            app.clamp_selection();
        }
        KeyCode::Home => {
            app.history.goto_oldest();
            app.clamp_selection();
        }
        KeyCode::End => {
            app.history.goto_live();
            app.clamp_selection();
        }

        KeyCode::Up | KeyCode::Char('k') => app.select_delta(-1),
        KeyCode::Down | KeyCode::Char('j') => app.select_delta(1),
        KeyCode::PageUp => app.select_delta(-10),
        KeyCode::PageDown => app.select_delta(10),

        // '=' so zooming out does not require Shift on most layouts.
        KeyCode::Char('+' | '=') => app.zoom_in(),
        KeyCode::Char('-' | '_') => app.zoom_out(),

        KeyCode::Char('s') => {
            app.sort = app.sort.next();
            app.selected = 0;
        }
        KeyCode::Char('i') => app.toggle_io(),
        KeyCode::Char('t') => {
            app.tree = !app.tree;
            app.selected = 0;
        }
        KeyCode::Char('/') => {
            app.editing_filter = true;
            app.filter.clear();
        }
        _ => {}
    }
}

// Keep `Sort` reachable for tests and future keybindings without a warning.
#[allow(dead_code)]
fn _assert_sort_cycles() {
    let _ = Sort::Cpu.next();
}
