//! ptop — a system monitor you can rewind.
//!
//! Copyright (C) 2026  ptop contributors
//!
//! This program is free software: you can redistribute it and/or modify it
//! under the terms of the GNU General Public License as published by the Free
//! Software Foundation, either version 3 of the License, or (at your option)
//! any later version. See the LICENSE file for details.

mod app;
mod collect;
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

    --glyphs=SET    timeline drawing: braille (default), block, or ascii.
                    Falls back to ascii automatically on a Linux console.
    --color=TIER    auto (default), mono, 16, 256, or true. Honours NO_COLOR.
    --theme=NAME    safe (default), classic, or auto. 'safe' replaces green with
                    cyan: green/yellow separates by only dE 3.7 under simulated
                    protanopia, against a target of 8, and red-green deficiency
                    affects roughly 8% of men. 'classic' restores green/yellow/red.

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

/// Wall-clock seconds between samples.
const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);
/// Samples retained, so ten minutes of scrollback at one per second.
const HISTORY_LEN: usize = 600;

/// Braille unless we are on a real Linux console, whose font has no braille
/// glyphs. btop makes the same check (`btop.cpp:815`).
fn default_glyphs() -> glyphs::GlyphSet {
    match std::env::var("TERM").as_deref() {
        Ok("linux") => glyphs::GlyphSet::Ascii,
        _ => glyphs::GlyphSet::Braille,
    }
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut collector = Platform::new()?;

    let mut glyphs = default_glyphs();
    let mut tier = theme::Tier::detect();
    let mut palette = theme::Palette::default();
    let mut positional = Vec::new();
    for a in &args {
        if let Some(name) = a.strip_prefix("--glyphs=") {
            match glyphs::GlyphSet::parse(name) {
                Some(set) => glyphs = set,
                None => {
                    eprintln!("ptop: unknown glyph set '{name}' (braille, block, ascii)");
                    std::process::exit(2);
                }
            }
        } else if let Some(name) = a.strip_prefix("--color=") {
            match name {
                // Re-detect rather than no-op, so a later --color=auto can
                // override an earlier --color= baked into a wrapper script.
                "auto" => tier = theme::Tier::detect(),
                _ => match theme::Tier::parse(name) {
                    Some(t) => tier = t,
                    None => {
                        eprintln!("ptop: unknown colour tier '{name}' (auto, mono, 16, 256, true)");
                        std::process::exit(2);
                    }
                },
            }
        } else if let Some(name) = a.strip_prefix("--theme=") {
            match name {
                // Re-resolve rather than no-op, so a later --theme=auto can
                // override an earlier --theme= baked into a wrapper script.
                "auto" | "default" => palette = theme::Palette::default(),
                _ => match theme::Palette::parse(name) {
                    Some(pal) => palette = pal,
                    None => {
                        eprintln!("ptop: unknown theme '{name}' (safe, classic, auto)");
                        std::process::exit(2);
                    }
                },
            }
        } else {
            positional.push(a.clone());
        }
    }
    let args = positional;

    match args.first().map(String::as_str) {
        Some("--once") => return once(&mut collector),
        Some("--bench") => {
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
                println!("{label}: {count} procs, {:?}/sample", t0.elapsed() / n);
            }
            return Ok(());
        }
        Some("--help" | "-h") => {
            println!("{USAGE}");
            return Ok(());
        }
        Some("--version" | "-V") => {
            println!("ptop {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some(other) => {
            eprintln!("ptop: unrecognised option '{other}'\n\n{USAGE}");
            std::process::exit(2);
        }
        None => {}
    }

    let mut app = App::new(HISTORY_LEN);
    app.glyphs = glyphs;
    app.theme = theme::Theme::new(palette, tier);

    // Collect once before drawing so the first frame has real numbers. CPU
    // still reads zero — there is no previous counter to diff against yet.
    app.push(collector.sample(app.needs())?);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &mut collector);
    ratatui::restore();
    result
}

/// Print one sample as plain text and exit.
///
/// Two samples are taken, one interval apart: CPU figures are deltas between
/// reads, so a single sample could only ever report zero.
fn once(collector: &mut impl Collector) -> io::Result<()> {
    let needs = Needs { io: true };
    collector.sample(needs)?;
    std::thread::sleep(SAMPLE_INTERVAL);
    let s = collector.sample(needs)?;

    println!(
        "cpu    {:.1}%  ({} cores)",
        s.cpu_total,
        s.cpu_per_core.len()
    );
    println!(
        "mem    {:.1}%  {} / {} used, {} available",
        s.mem.used_pct(),
        human(s.mem.used),
        human(s.mem.total),
        human(s.mem.available)
    );
    if s.mem.swap_total > 0 {
        println!(
            "swap   {:.1}%  {} / {}",
            s.mem.swap_pct(),
            human(s.mem.swap_used),
            human(s.mem.swap_total)
        );
    }
    println!("load   {:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2]);
    println!("procs  {}", s.procs.len());
    if s.io_denied > 0 {
        println!(
            "io     {}/{} processes unreadable — run as root to see them",
            s.io_denied,
            s.procs.len()
        );
    }

    let mut top = s.procs.clone();
    top.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    println!(
        "\n{:>7}  {:>6}  {:>9}  {:>10}  {:>10}  COMMAND",
        "PID", "CPU%", "RSS", "DISK R/s", "DISK W/s"
    );
    for p in top.iter().take(10) {
        // A dash, never a zero: this process could not be read, which is not
        // the same as it doing no IO.
        let (r, w) = match p.io {
            Some(io) => (human(io.read), human(io.write)),
            None => ("—".into(), "—".into()),
        };
        println!(
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
    let mut last_sample = Instant::now();

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll with whatever is left of the sample interval: input stays
        // responsive without spinning, and sampling stays on schedule.
        let timeout = SAMPLE_INTERVAL.saturating_sub(last_sample.elapsed());
        if event::poll(timeout)?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key.code, key.modifiers);
        }

        if last_sample.elapsed() >= SAMPLE_INTERVAL {
            // Sampling continues while paused — that is the whole point. The
            // cursor stays put, the buffer keeps filling behind it.
            app.push(collector.sample(app.needs())?);
            app.clamp_selection();
            last_sample = Instant::now();
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
