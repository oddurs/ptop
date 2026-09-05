//! Headless render tests.
//!
//! A TUI that panics on a 20-column terminal or an empty buffer is worse than
//! no TUI, and neither case shows up in normal use. `TestBackend` renders into
//! a plain buffer so both are cheap to exercise.

use crate::app::App;
use crate::sample::{MemStat, ProcSample, Sample};
use crate::theme::{Palette, Theme, Tier};
use crate::ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn proc_named(pid: i32, name: &str, cpu: f32, rss: u64) -> ProcSample {
    ProcSample {
        pid,
        ppid: 1,
        name: name.into(),
        user: std::sync::Arc::from("root"),
        cpu,
        rss,
        threads: 1,
        state: 'S',
        io: None,
    }
}

fn sample(cpu: f32) -> Sample {
    sample_at(cpu, 0)
}

/// `age_secs` back-dates the sample, so tests can exercise anything that reads
/// the clock rather than counting rows.
fn sample_at(cpu: f32, age_secs: u64) -> Sample {
    Sample {
        at: std::time::SystemTime::now() - std::time::Duration::from_secs(age_secs),
        cpu_total: cpu,
        cpu_per_core: vec![cpu, cpu / 2.0, 0.0, 99.0],
        mem: MemStat {
            total: 16 << 30,
            used: 8 << 30,
            available: 8 << 30,
            swap_total: 2 << 30,
            swap_used: 1 << 30,
        },
        load: [1.0, 2.0, 3.0],
        procs: vec![
            proc_named(1, "init", 0.1, 1 << 20),
            proc_named(42, "postgres", 88.0, 512 << 20),
            proc_named(99, "nginx", 12.5, 32 << 20),
        ],
        uptime: std::time::Duration::from_secs(90_000),
        io_collected: false,
        io_denied: 0,
    }
}

fn render(app: &App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, app)).unwrap();
    term.backend()
        .buffer()
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect::<String>()
}

#[test]
fn renders_without_panicking() {
    let mut app = App::new(60);
    app.push(sample(42.0));
    let out = render(&app, 100, 30);
    assert!(out.contains("postgres"));
    assert!(out.contains("LIVE"));
}

#[test]
fn empty_history_shows_placeholder_instead_of_panicking() {
    let app = App::new(60);
    let out = render(&app, 80, 24);
    assert!(out.contains("collecting"));
}

#[test]
fn survives_absurdly_small_terminal() {
    let mut app = App::new(60);
    app.push(sample(50.0));
    // Every panel is narrower than its own title here.
    for (w, h) in [(20, 10), (10, 6), (4, 3), (1, 1)] {
        render(&app, w, h);
    }
}

#[test]
fn paused_state_is_visibly_marked() {
    let mut app = App::new(60);
    // Oldest first, one second apart, newest last.
    for i in (0..10).rev() {
        app.push(sample_at((9 - i) as f32 * 10.0, i));
    }
    app.history.scrub(-4);
    let out = render(&app, 100, 30);
    assert!(out.contains("PAUSED"), "paused view must not look live");
    assert!(
        out.contains("-4s"),
        "paused badge must report real elapsed lag"
    );
    assert!(
        out.contains('▌') || out.contains('▐'),
        "scrub cursor must be visible without colour"
    );
}

#[test]
fn filter_narrows_the_table() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    app.filter = "postgres".into();
    let out = render(&app, 100, 30);
    assert!(out.contains("postgres"));
    assert!(!out.contains("nginx"));
}

#[test]
fn sort_by_mem_puts_the_biggest_process_first() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    app.sort = crate::app::Sort::Mem;
    let names: Vec<&str> = app
        .visible_rows()
        .iter()
        .map(|r| r.proc.name.as_str())
        .collect();
    assert_eq!(names, vec!["postgres", "nginx", "init"]);
}

#[test]
fn selection_survives_the_list_shrinking_under_it() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    app.selected = 2;
    // A filter that leaves a single row would otherwise strand the cursor
    // past the end of the table.
    app.filter = "nginx".into();
    app.clamp_selection();
    assert_eq!(app.selected, 0);
    render(&app, 100, 30);
}

#[test]
#[ignore = "visual check: cargo test -- --ignored --nocapture show_frame"]
fn show_frame() {
    let mut app = App::new(600);
    for i in 0..300 {
        let t = i as f32;
        let mut s = sample_at((t * 0.7).sin().abs() * 95.0, 300 - i);
        s.mem.used = ((8.0 + (t * 0.2).sin() * 3.0) as u64) << 30;
        app.push(s);
    }
    app.history.scrub(-18);

    for (label, zoom_steps, set) in [
        ("braille, zoom 1", 0, crate::glyphs::GlyphSet::Braille),
        ("braille, zoomed out", 3, crate::glyphs::GlyphSet::Braille),
        ("ascii fallback", 0, crate::glyphs::GlyphSet::Ascii),
    ] {
        let mut a = App::new(600);
        for i in 0..300 {
            let t = i as f32;
            let mut s = sample_at((t * 0.7).sin().abs() * 95.0, 300 - i);
            s.mem.used = ((8.0 + (t * 0.2).sin() * 3.0) as u64) << 30;
            a.push(s);
        }
        a.history.scrub(-18);
        a.glyphs = set;
        for _ in 0..zoom_steps {
            a.zoom_out();
        }
        println!("\n=== {label} ===");
        let mut term = Terminal::new(TestBackend::new(100, 12)).unwrap();
        term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &a))
            .unwrap();
        let buf = term.backend().buffer();
        for y in 0..buf.area.height {
            let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            println!("{}", row.trim_end());
        }
    }
}

#[test]
fn cursor_marker_picks_the_correct_half_of_a_cell() {
    // Braille packs two samples per cell, so the marker has to distinguish
    // them or scrubbing loses half its precision.
    let mut app = App::new(60);
    for i in (0..8).rev() {
        app.push(sample_at(10.0, i));
    }
    app.glyphs = crate::glyphs::GlyphSet::Braille;

    // Newest is slot 7 (right half of cell 3); one back is slot 6 (left half).
    app.history.scrub(-1);
    assert!(
        render(&app, 100, 30).contains('▌'),
        "odd offset is a left half"
    );
    app.history.scrub(-1);
    assert!(
        render(&app, 100, 30).contains('▐'),
        "even offset is a right half"
    );
}

#[test]
fn ascii_glyphs_render_without_any_unicode() {
    let mut app = App::new(60);
    for i in (0..20).rev() {
        app.push(sample_at(80.0, i));
    }
    app.glyphs = crate::glyphs::GlyphSet::Ascii;
    app.history.scrub(-3);
    let out = render(&app, 100, 30);
    // Box-drawing borders are ratatui's; the graph area itself must be plain.
    assert!(out.contains('#'), "ascii set must draw a filled bar");
    assert!(!out.contains('⣿'), "ascii set must not emit braille");
    assert!(out.contains('^'), "ascii cursor marker");
}

#[test]
fn zooming_out_widens_the_time_span_shown() {
    let mut app = App::new(600);
    for i in (0..400).rev() {
        app.push(sample_at(50.0, i));
    }
    // At zoom 1 a 40-column terminal cannot show 400 samples; zoomed out it can.
    let narrow = 40;
    let at_zoom_1 = render(&app, narrow, 30);
    for _ in 0..4 {
        app.zoom_out();
    }
    let at_max_zoom = render(&app, narrow, 30);
    assert!(app.zoom() > 1);
    assert_ne!(at_zoom_1, at_max_zoom, "zoom must change what is drawn");
}

#[test]
fn zoom_is_clamped_at_both_ends() {
    let mut app = App::new(60);
    for _ in 0..20 {
        app.zoom_in();
    }
    assert_eq!(app.zoom(), crate::app::ZOOM_LEVELS[0]);
    for _ in 0..20 {
        app.zoom_out();
    }
    assert_eq!(app.zoom(), *crate::app::ZOOM_LEVELS.last().unwrap());
}

#[test]
fn a_spike_survives_aggregation_at_every_zoom_level() {
    // The core promise: zooming out must never hide a spike.
    for &z in crate::app::ZOOM_LEVELS.iter() {
        let mut app = App::new(600);
        for i in (0..120).rev() {
            // One 100% sample buried in otherwise idle history.
            app.push(sample_at(if i == 60 { 100.0 } else { 0.0 }, i));
        }
        while app.zoom() < z {
            app.zoom_out();
        }
        let out = render(&app, 100, 30);
        assert!(
            out.contains('⣿') || out.contains('⡇') || out.contains('⢸'),
            "spike vanished at zoom {z}"
        );
    }
}

#[test]
fn zoom_is_clamped_to_what_the_buffer_can_fill() {
    use crate::app::effective_zoom;
    // 300 samples over 196 slots needs 2 per slot; asking for 8 would shrink
    // the graph into a corner and leave most of the width blank.
    assert_eq!(effective_zoom(8, 300, 196), 2);
    assert_eq!(effective_zoom(8, 600, 196), 4);
    // A narrow terminal genuinely needs the higher levels.
    assert_eq!(effective_zoom(8, 600, 80), 8);
    // Never below 1, and never divides by zero.
    assert_eq!(effective_zoom(1, 0, 196), 1);
    assert_eq!(effective_zoom(4, 600, 0), 4);
}

#[test]
fn timeline_fills_its_panel_with_no_blank_rows() {
    // The acceptance test for L1. Its bounds previously skipped the border
    // rows and column — which after L1 are exactly the space the change
    // reclaimed, so a regression leaving the last row blank would have passed.
    let (w, h) = (60u16, 10u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &app))
        .unwrap();
    let buf = term.backend().buffer();

    // Every row, including the first (the section rule) and the last.
    for y in 0..h {
        let row: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
        assert!(
            row.chars().any(|c| c != ' '),
            "row {y} of the timeline is blank:\n{row:?}"
        );
    }
    // And column 0, which used to be a border, now carries content.
    let col0: String = (1..h).map(|y| buf[(0, y)].symbol()).collect();
    assert!(
        col0.chars().any(|c| c != ' '),
        "column 0 is unused: {col0:?}"
    );
}

#[test]
fn tree_mode_renders_nesting_in_the_table() {
    let mut app = App::new(60);
    let mut s = sample(10.0);
    // postgres(42) parents nginx(99); init(1) parents postgres.
    s.procs = vec![
        proc_named(1, "init", 0.1, 1 << 20),
        proc_named(42, "postgres", 88.0, 512 << 20),
        proc_named(99, "nginx", 12.5, 32 << 20),
    ];
    s.procs[1].ppid = 1;
    s.procs[2].ppid = 42;
    app.push(s);
    app.tree = true;

    let out = render(&app, 100, 30);
    assert!(out.contains("tree"), "title should say the tree is on");
    assert!(out.contains("└─ postgres") || out.contains("├─ postgres"));
    assert!(out.contains("nginx"));
}

#[test]
fn tree_mode_keeps_every_process_visible() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    let flat = app.visible_rows().len();
    app.tree = true;
    assert_eq!(app.visible_rows().len(), flat, "tree must not drop rows");
}

#[test]
fn tree_survives_a_tiny_terminal() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    app.tree = true;
    for (w, h) in [(20, 10), (4, 3), (1, 1)] {
        render(&app, w, h);
    }
}

#[test]
#[ignore = "visual check: cargo test -- --ignored --nocapture show_real_tree"]
fn show_real_tree() {
    use crate::collect::{Collector, Platform};
    let mut c = Platform::new().unwrap();
    let mut app = App::new(60);
    app.push(c.sample(Default::default()).unwrap());
    app.tree = true;
    app.sort = crate::app::Sort::Pid;

    let rows = app.visible_rows();
    println!(
        "{} processes, {} rows",
        app.history.current().unwrap().procs.len(),
        rows.len()
    );
    let depth = |r: &crate::tree::TreeRow| r.prefix.chars().count() / 3;
    println!("max depth: {}", rows.iter().map(depth).max().unwrap_or(0));
    println!(
        "roots: {}",
        rows.iter().filter(|r| r.prefix.is_empty()).count()
    );
    for r in rows.iter().take(28) {
        println!("{:>7} {}{}", r.proc.pid, r.prefix, r.proc.name);
    }
}

#[test]
fn io_columns_appear_only_when_asked_for() {
    let mut app = App::new(60);
    app.push(sample(10.0));
    assert!(!render(&app, 120, 30).contains("DISK"));
    app.toggle_io();
    assert!(render(&app, 120, 30).contains("DISK"));
}

#[test]
fn io_collection_is_a_ratchet() {
    use crate::collect::Needs;
    let mut app = App::new(60);
    assert_eq!(app.needs(), Needs { io: false });

    app.toggle_io();
    assert_eq!(app.needs(), Needs { io: true });

    // Hiding the columns must NOT stop collection: resuming later would leave
    // a hole in the middle of history rather than one clean boundary.
    app.toggle_io();
    assert!(!app.show_io);
    assert_eq!(app.needs(), Needs { io: true }, "collection must not stop");
}

#[test]
fn history_without_io_says_so_rather_than_showing_zero() {
    let mut app = App::new(60);
    let mut old = sample(10.0);
    old.io_collected = false; // recorded before the column was switched on
    app.push(old);
    app.toggle_io();

    let out = render(&app, 120, 30);
    assert!(out.contains("not collected"), "must explain the blank");
    assert!(out.contains('·'), "blank marker, never a fabricated 0");
}

#[test]
fn unreadable_processes_are_blank_not_zero() {
    let mut app = App::new(60);
    let mut s = sample(10.0);
    s.io_collected = true;
    s.procs[0].io = Some(crate::sample::IoRates {
        read: 2048,
        write: 0,
    });
    // procs[1] and [2] stay None: readable by root only.
    s.io_denied = 2;
    app.push(s);
    app.toggle_io();

    let out = render(&app, 120, 30);
    assert!(out.contains("2.0K/s"), "a real rate renders as a rate");
    assert!(out.contains('—'), "unreadable renders as a dash");
    assert!(
        out.contains("2/3 need root"),
        "title should explain the dashes"
    );
}

#[test]
fn only_unreadable_processes_prompt_for_root() {
    // A process merely awaiting its second reading also shows a dash, but it
    // resolves on its own; blaming permissions for that case would be wrong.
    let mut app = App::new(60);
    let mut s = sample(10.0);
    s.io_collected = true;
    s.io_denied = 0; // all readable, none has a prior counter yet
    app.push(s);
    app.toggle_io();

    let out = render(&app, 120, 30);
    assert!(out.contains('—'), "no prior reading still shows a dash");
    assert!(!out.contains("need root"), "but must not blame permissions");
}

#[test]
#[ignore = "digest for cross-branch comparison: cargo test -- --ignored --nocapture render_digest"]
fn render_digest() {
    // Symbol *and* style per cell, so a colour change shows up. Used to prove
    // a refactor is visually a no-op.
    //
    // Deliberately hermetic: timestamps come from a fixed epoch rather than
    // `now()`, because the header renders elapsed lag and a scheduler stall
    // mid-loop would tick it over a second and change the hash — a false
    // "rendering changed" verdict during exactly the comparison this exists
    // for.
    //
    // Also renders every state that owns a distinct token. A digest that never
    // enters the live branch or the filter prompt would stay unchanged if
    // someone repainted them, and would then be quietly lying.
    fn fixed_sample(cpu: f32, age_secs: u64) -> Sample {
        let mut s = sample(cpu);
        s.at = std::time::SystemTime::UNIX_EPOCH
            + std::time::Duration::from_secs(1_700_000_000 - age_secs);
        s.io_collected = true;
        s.io_denied = 1;
        s
    }

    let build = || {
        let mut app = App::new(600);
        for i in (0..120).rev() {
            app.push(fixed_sample((i as f32 * 0.7).sin().abs() * 95.0, i));
        }
        app
    };

    let mut out = String::new();
    for (label, prep) in [("paused+tree+io", 0u8), ("live", 1), ("filter-prompt", 2)] {
        let mut app = build();
        match prep {
            0 => {
                app.history.scrub(-9);
                app.toggle_io();
                app.tree = true;
            }
            1 => {} // stays live: exercises theme.live
            _ => {
                app.editing_filter = true;
                app.filter = "pg".into();
            }
        }
        let mut term = Terminal::new(TestBackend::new(110, 34)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();
        out.push_str(label);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let c = &buf[(x, y)];
                out.push_str(&format!(
                    "{}|{:?}|{:?}|{:?};",
                    c.symbol(),
                    c.fg,
                    c.bg,
                    c.modifier
                ));
            }
        }
    }

    // A hash keeps the output to one line; any cell difference changes it.
    let mut h: u64 = 1469598103934665603;
    for b in out.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    println!("RENDER_DIGEST {h:016x}");
}

#[test]
fn mono_tier_emits_no_colour_anywhere_on_screen() {
    // The acceptance criterion for the tier: not "mostly grey", but that no
    // rendered cell carries a colour at all.
    let mut app = App::new(60);
    let mut s = sample(90.0);
    s.io_collected = true;
    s.io_denied = 1;
    app.push(s);
    app.theme = Theme::new(Palette::Classic, Tier::Mono);
    app.toggle_io();
    app.tree = true;
    app.history.scrub(-1);

    let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            let c = &buf[(x, y)];
            assert_eq!(
                (c.fg, c.bg),
                (ratatui::style::Color::Reset, ratatui::style::Color::Reset),
                "cell ({x},{y}) {:?} carries colour at the mono tier",
                c.symbol()
            );
        }
    }
}

#[test]
fn mono_tier_still_marks_the_paused_state() {
    // Losing colour must not lose the loudest warning in the UI.
    let mut app = App::new(60);
    for i in (0..6).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Classic, Tier::Mono);
    app.history.scrub(-3);

    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    let reversed = (0..buf.area.width).any(|x| {
        buf[(x, 0)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    });
    assert!(reversed, "PAUSED badge must stay loud without colour");
}

#[test]
fn every_tier_renders() {
    for tier in [Tier::Mono, Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
        let mut app = App::new(60);
        app.push(sample(75.0));
        app.theme = Theme::new(Palette::Classic, tier);
        let out = render(&app, 100, 30);
        assert!(out.contains("postgres"), "{tier:?} failed to render");
        // And at a size where everything is fighting for room.
        render(&app, 20, 8);
    }
}

#[test]
fn coloured_tiers_actually_differ_from_mono() {
    // Guards against a tier that silently resolves to the same styling, which
    // would make the mono test above pass for the wrong reason.
    let styled = |tier| {
        let mut app = App::new(60);
        app.push(sample(90.0));
        app.theme = Theme::new(Palette::Classic, tier);
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();
        (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .map(|(x, y)| format!("{:?}{:?}", buf[(x, y)].fg, buf[(x, y)].bg))
            .collect::<String>()
    };
    let mono = styled(Tier::Mono);
    assert_ne!(mono, styled(Tier::Ansi16));
    assert_ne!(mono, styled(Tier::TrueColor));
    assert_ne!(styled(Tier::Ansi16), styled(Tier::TrueColor));
}

#[test]
#[ignore = "visual check: cargo test -- --ignored --nocapture show_tiers"]
fn show_tiers() {
    // Prints the frame with a modifier map under the header, so the monochrome
    // tier can be eyeballed for whether meaning survives without colour.
    use ratatui::style::Modifier;
    for tier in [Tier::Mono, Tier::TrueColor] {
        let mut app = App::new(600);
        for i in (0..200).rev() {
            app.push(sample_at((i as f32 * 0.7).sin().abs() * 95.0, i));
        }
        app.theme = Theme::new(Palette::Classic, tier);
        app.history.scrub(-8);
        let mut term = Terminal::new(TestBackend::new(96, 26)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();
        println!("\n=== {tier:?} ===");
        for y in 0..buf.area.height {
            let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            println!("{}", row.trim_end());
            if y < 2 {
                let mods: String = (0..buf.area.width)
                    .map(|x| {
                        let m = buf[(x, y)].modifier;
                        if m.contains(Modifier::REVERSED) {
                            'R'
                        } else if m.contains(Modifier::BOLD) {
                            'B'
                        } else if m.contains(Modifier::DIM) {
                            'd'
                        } else {
                            ' '
                        }
                    })
                    .collect();
                if !mods.trim().is_empty() {
                    println!("{}", mods.trim_end());
                }
            }
        }
    }
}

#[test]
fn the_default_theme_is_the_colour_vision_safe_one() {
    // The point of C3: safe is the default, classic is the escape hatch, not
    // the other way round.
    let app = App::new(60);
    assert_eq!(app.theme, Theme::default());
    assert_eq!(Palette::default(), Palette::Safe);
    assert_ne!(app.theme.ok, ratatui::style::Color::Green);
}

#[test]
fn every_section_rule_uses_the_chrome_token() {
    // Replaces the panel-border test: L1 removed the boxes. Identifies a rule
    // by its shape — a row that is mostly `─` — rather than by hardcoded row
    // numbers, so it keeps working as sections move. The tree spine also draws
    // `─`, but only a glyph or two per row, so the majority test excludes it.
    //
    // Tree mode is on for exactly that reason.
    let mut app = App::new(60);
    app.push(sample(50.0));
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.tree = true;

    let (w, h) = (100u16, 30u16);
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();

    let mut rules = 0;
    for y in 0..h {
        let dashes = (0..w).filter(|&x| buf[(x, y)].symbol() == "─").count();
        if dashes * 2 < w as usize {
            continue;
        }
        rules += 1;
        for x in 0..w {
            let c = &buf[(x, y)];
            if c.symbol() == "─" {
                assert_eq!(
                    c.fg, app.theme.chrome,
                    "section rule at ({x},{y}) is not chrome-coloured"
                );
            }
        }
    }
    // Timeline and processes. The cores section folded into the header in L2,
    // so it has no rule of its own any more.
    assert!(rules >= 2, "expected a rule per section, saw {rules}");
}

#[test]
fn the_tree_spine_recedes_like_a_gridline() {
    // The spine is structure, not data. It shares a cell with the process
    // name, so this checks the two are styled differently rather than the
    // whole cell inheriting one style.
    let mut app = App::new(60);
    let mut s = sample(10.0);
    s.procs[1].ppid = 1;
    s.procs[2].ppid = 42;
    app.push(s);
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.tree = true;

    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();

    let mut spine = 0;
    for y in 0..30 {
        // Column 0 is the panel edge; the spine lives inside the COMMAND cell.
        for x in 1..99u16 {
            let c = &buf[(x, y)];
            // Both halves of the prefix. They share a span today, but the
            // test should state what its name claims rather than rely on that.
            if matches!(c.symbol(), "├" | "└" | "─") {
                // Only the spine inside the COMMAND column, not section rules.
                if x > 40 {
                    spine += 1;
                    assert_eq!(
                        c.fg, app.theme.chrome,
                        "tree spine at ({x},{y}) is not chrome-coloured"
                    );
                }
            }
        }
    }
    assert!(spine > 0, "expected a tree spine to be drawn, saw none");
}

/// Rows of the timeline panel that contain a chrome-styled glyph, excluding the
/// panel edges. Used to locate the threshold rules precisely.
fn rule_rows(app: &App, w: u16, h: u16) -> Vec<usize> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), app))
        .unwrap();
    let buf = term.backend().buffer();
    (1..h)
        .filter(|&y| {
            (0..w).any(|x| {
                let c = &buf[(x, y)];
                c.fg == app.theme.chrome && c.symbol() != " "
            })
        })
        .map(|y| (y - 1) as usize)
        .collect()
}

#[test]
fn the_rules_land_on_exactly_the_threshold_rows() {
    // The previous version of this test asserted only that *some* cell was
    // chrome-coloured — which the panel border satisfies, so it passed with
    // the rule removed entirely. This one pins the exact rows, so it cannot.
    let (w, h) = (100u16, 12u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        app.push(sample_at(10.0, i)); // data confined to the bottom row
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    // Same arithmetic the renderer uses, so the expectation tracks the layout.
    let graph_rows = (h as usize - 1).saturating_sub(2).max(1);
    let cpu_rows = (graph_rows * 3 / 5).max(1);
    let mem_rows = graph_rows - cpu_rows;

    let mut expected: Vec<usize> = Vec::new();
    for pct in [
        crate::theme::Theme::WARN_PCT,
        crate::theme::Theme::CRITICAL_PCT,
    ] {
        if let Some((r, _)) = crate::glyphs::rule_position(pct, cpu_rows) {
            expected.push(r);
        }
    }
    for pct in [
        crate::theme::Theme::WARN_PCT,
        crate::theme::Theme::CRITICAL_PCT,
    ] {
        if let Some((r, _)) = crate::glyphs::rule_position(pct, mem_rows) {
            expected.push(cpu_rows + r);
        }
    }
    expected.sort_unstable();
    expected.dedup();

    assert_eq!(rule_rows(&app, w, h), expected);
    assert!(
        !expected.is_empty(),
        "no rules expected — test proves nothing"
    );
}

#[test]
fn both_thresholds_get_a_rule_not_just_critical() {
    // The warn boundary is the one the roadmap asked for; it was hue-only.
    let (w, h) = (100u16, 16u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        let mut s = sample_at(5.0, i);
        s.mem.used = 1 << 30; // low too, or the 50% band holds data legitimately
        app.push(s);
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    let rows = rule_rows(&app, w, h);
    assert!(
        rows.len() >= 4,
        "expected warn and critical rules in both graphs, found rows {rows:?}"
    );
}

#[test]
fn the_rule_is_dashed_so_it_cannot_be_read_as_data() {
    // At the mono tier chrome and a low bar are both dim, and a solid rule row
    // renders the same glyph a level-1 bar does. Dashing is what separates a
    // reference line from a row of samples when colour is unavailable.
    let (w, h) = (100u16, 12u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        app.push(sample_at(10.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::Mono);

    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &app))
        .unwrap();
    let buf = term.backend().buffer();

    let rule_y = 1 + crate::glyphs::rule_position(
        crate::theme::Theme::CRITICAL_PCT,
        (((h as usize - 1).saturating_sub(2)).max(1) * 3 / 5).max(1),
    )
    .unwrap()
    .0 as u16;

    // U+2800 is the braille blank: visually empty, but not an ASCII space, so
    // it must be counted as a gap or every braille row looks solid.
    let blank = |s: &str| s == " " || s == "\u{2800}";
    let row: Vec<String> = (1..w - 1)
        .map(|x| buf[(x, rule_y)].symbol().to_string())
        .collect();
    let marks = row.iter().filter(|s| !blank(s)).count();
    let blanks = row.iter().filter(|s| blank(s)).count();
    assert!(marks > 10, "rule row has no marks: {marks}");
    assert!(
        blanks > 10,
        "rule row is solid, indistinguishable from a bar at the mono tier"
    );
}

#[test]
fn data_always_wins_the_cell_over_the_rule() {
    // An earlier version OR'd the rule into the bar glyph, so a cell holding a
    // spike and an idle sample lit a dot at the rule height in the data
    // colour — identical to the idle sample having crossed the threshold.
    let (w, h) = (100u16, 12u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        let mut s = sample_at(100.0, i);
        s.mem.used = s.mem.total; // both graphs full, so no cell is empty
        app.push(s);
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    assert!(
        rule_rows(&app, w, h).is_empty(),
        "rule drew over cells that contain data"
    );
}

/// Column of the scrub cursor marker in a rendered timeline, if drawn.
fn cursor_column(app: &App, w: u16, h: u16) -> Option<u16> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), app))
        .unwrap();
    let buf = term.backend().buffer();
    for y in 0..h {
        for x in 0..w {
            if matches!(buf[(x, y)].symbol(), "▌" | "▐" | "^") {
                return Some(x);
            }
        }
    }
    None
}

#[test]
fn the_cursor_stays_over_its_own_column_once_the_gutter_exists() {
    // The gutter shifts the graph right; if the cursor row is not padded by
    // the same amount the marker points four columns off the sample it claims.
    //
    // Asserted as an exact column, computed the way the renderer computes it.
    // An approximate assertion ("right of the gutter", "past halfway") passed
    // happily with the padding removed — verified — which is no test at all.
    let (w, h) = (100u16, 12u16);
    let n = 40usize;
    let mut app = App::new(600);
    for i in (0..n).rev() {
        app.push(sample_at(50.0, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.scrub(-7);

    let gutter = 4usize;
    let graph_w = w as usize - gutter;
    let spc = app.glyphs.samples_per_cell();
    let slots = graph_w * spc;
    let zoom = crate::app::effective_zoom(app.zoom(), n, slots);
    let shown = (slots * zoom).min(n);
    let dropped = app.history.len() - shown;
    let idx = app.history.cursor_index() - dropped;
    let slot = crate::history::slot_of_index(idx, shown, zoom, slots);
    let expected = gutter as u16 + (slot / spc) as u16;

    assert_eq!(
        cursor_column(&app, w, h),
        Some(expected),
        "cursor marker is not over the sample it points at"
    );
}

/// The gutter columns of every graph row, as one string.
///
/// Scoped to the gutter rather than the whole frame: asserting on the full
/// render made the narrow case depend on the panel title never containing the
/// digits "100", which is unrelated to what the test is about.
fn gutter_text(app: &App, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), app))
        .unwrap();
    let buf = term.backend().buffer();
    let graph_rows = (h as usize - 1).saturating_sub(2).max(1);
    (0..graph_rows)
        .map(|row| {
            (0..4u16.min(w))
                .map(|x| buf[(x, 1 + row as u16)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_gutter_carries_the_scale_and_yields_on_a_narrow_panel() {
    let mut app = App::new(600);
    for i in (0..50).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let wide = gutter_text(&app, 100, 12);
    assert!(
        wide.contains("100"),
        "wide panel lost the top anchor:\n{wide}"
    );
    assert!(
        wide.contains('0'),
        "wide panel lost the zero anchor:\n{wide}"
    );

    // Four columns of axis is a poor trade against four columns of history
    // when there is barely any room.
    let narrow = gutter_text(&app, 24, 12);
    assert!(
        !narrow.contains("100"),
        "narrow panel should drop the gutter, got:\n{narrow}"
    );
}

#[test]
fn a_section_too_short_for_both_ends_carries_no_axis_at_all() {
    // A one-row section spans the whole 0..100 range. Labelling its top `100`
    // implies the bottom is not zero, and `0` never appears anywhere — the
    // axis states something false rather than merely being absent.
    //
    // Checked per section: the two graphs are sized independently, so a short
    // CPU section can sit above a MEM section that legitimately has a scale.
    let mut app = App::new(600);
    for i in (0..50).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    for h in [5u16, 6, 7, 8, 12] {
        let rows: Vec<String> = gutter_text(&app, 60, h)
            .lines()
            .map(str::to_string)
            .collect();
        let graph_rows = (h as usize - 1).saturating_sub(2).max(1);
        let cpu_rows = (graph_rows * 3 / 5).max(1);
        let mem_rows = graph_rows - cpu_rows;

        for (name, range, n) in [
            ("cpu", 0..cpu_rows, cpu_rows),
            ("mem", cpu_rows..graph_rows, mem_rows),
        ] {
            let section: String = rows.get(range).unwrap_or_default().join("");
            let labelled = section.contains("100") || section.contains('0');
            assert_eq!(
                labelled,
                n >= 2,
                "h={h}: {name} section of {n} row(s) labelled={labelled}, in:\n{section}"
            );
        }
    }
}

#[test]
fn the_gutter_never_overlaps_the_graph() {
    // Every graph row must start with the gutter, so no glyph can be drawn
    // under the axis labels.
    let (w, h) = (100u16, 12u16);
    let mut app = App::new(600);
    for i in (0..200).rev() {
        app.push(sample_at(100.0, i)); // saturated: bars everywhere
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &app))
        .unwrap();
    let buf = term.backend().buffer();

    let graph_rows = (h as usize - 1).saturating_sub(2).max(1);
    for row in 0..graph_rows {
        let y = 1 + row as u16;
        for x in 0..4u16 {
            let s = buf[(x, y)].symbol();
            // The concrete allowed set, not "any alphanumeric": a future glyph
            // set that used a letter would otherwise pass this silently.
            const ALLOWED: [&str; 9] = [" ", "0", "1", "C", "P", "U", "M", "E", "0"];
            assert!(
                ALLOWED.contains(&s),
                "unexpected glyph {s:?} inside the gutter at ({x},{y})"
            );
        }
    }
}

#[test]
fn the_gutter_names_each_series_directly() {
    // A direct label beats a legend: the reader stops having to hold
    // "top is cpu" in their head while reading the graph.
    let mut app = App::new(600);
    for i in (0..50).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let g = gutter_text(&app, 100, 12);
    assert!(g.contains("CPU"), "cpu graph is unlabelled:\n{g}");
    assert!(g.contains("MEM"), "mem graph is unlabelled:\n{g}");
}

#[test]
fn the_legend_keeps_identifying_the_series_when_the_gutter_cannot() {
    // The identification has to live somewhere. When a section is too short to
    // carry a label, dropping the legend line too would leave the reader with
    // two anonymous graphs.
    let mut app = App::new(600);
    for i in (0..50).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let whole = |w: u16, h: u16| {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &app))
            .unwrap();
        let buf = term.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };

    // Tall: gutter labels present, legend sheds its identification half.
    let tall = whole(100, 14);
    assert!(tall.contains("CPU"));
    assert!(
        !tall.contains("cpu · mem"),
        "legend duplicates the gutter label"
    );

    // Narrow: no gutter at all, so the legend must still say which is which.
    let narrow = whole(24, 14);
    assert!(
        !narrow.contains("CPU"),
        "narrow panel should have no gutter"
    );
    assert!(
        narrow.contains("cpu · mem"),
        "narrow panel dropped both the label and the legend:\n{narrow}"
    );
}

#[test]
#[ignore = "regenerates the README sample frame"]
fn readme_frame() {
    let mut app = App::new(600);
    for i in 0..300 {
        let x = i as f32;
        let mut s = sample_at((x * 0.7).sin().abs() * 95.0, 300 - i);
        s.mem.used = ((8.0 + (x * 0.2).sin() * 3.0) as u64) << 30;
        s.procs = vec![
            proc_named(1, "systemd", 0.1, 12 << 20),
            proc_named(824, "postgres", 88.4, 512 << 20),
            proc_named(1190, "nginx", 12.5, 32 << 20),
            proc_named(2077, "node", 4.2, 148 << 20),
        ];
        app.push(s);
    }
    app.history.scrub(-18);
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    let mut term = Terminal::new(TestBackend::new(78, 20)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    for y in 0..buf.area.height {
        let row: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        println!("{}", row.trim_end());
    }
}

/// The whole timeline panel as text, one string per row.
fn timeline_rows(app: &App, w: u16, h: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), app))
        .unwrap();
    let buf = term.backend().buffer();
    (0..h)
        .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect()
}

#[test]
fn the_cursor_reports_the_values_at_the_cursor_not_the_live_ones() {
    // The whole point of the readout. If it showed the newest sample it would
    // contradict the process table beside it, which does follow the cursor.
    let mut app = App::new(600);
    // Oldest 90%, newest 10%, so the two are impossible to confuse.
    for i in (0..40).rev() {
        app.push(sample_at(if i > 20 { 90.0 } else { 10.0 }, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.scrub(-35); // back into the 90% region

    let text = timeline_rows(&app, 100, 12).join("\n");
    assert!(
        text.contains("CPU 90.0%"),
        "readout shows the live value, not the cursor's:\n{text}"
    );
    assert!(!text.contains("CPU 10.0%"));
}

#[test]
fn the_readout_is_absent_while_live() {
    let mut app = App::new(600);
    for i in (0..40).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let live = timeline_rows(&app, 100, 12).join("\n");
    assert!(!live.contains("CPU 50.0%"), "readout shown while live");
    assert!(live.contains("past"), "live row should show the time axis");

    app.history.scrub(-5);
    assert!(
        timeline_rows(&app, 100, 12)
            .join("\n")
            .contains("CPU 50.0%")
    );
}

#[test]
fn the_readout_never_pushes_the_marker_off_its_column() {
    // Asserting `row.len() == w` cannot fail: TestBackend is a fixed grid
    // pre-filled with spaces and ratatui truncates an over-wide line, so an
    // overflowing row measures `w` either way. The observable consequence of
    // overflow is the marker sliding, so assert the marker's column directly.
    for w in [40u16, 60, 80, 100, 140] {
        for back in [1usize, 5, 20, 60] {
            let n = 80usize;
            let mut app = App::new(600);
            for i in (0..n).rev() {
                app.push(sample_at(50.0, i as u64));
            }
            app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
            app.history.scrub(-(back as isize));

            let gutter = if w as usize >= 30 { 4 } else { 0 };
            let graph_w = w as usize - gutter;
            let spc = app.glyphs.samples_per_cell();
            let slots = graph_w * spc;
            let zoom = crate::app::effective_zoom(app.zoom(), n, slots);
            let shown = (slots * zoom).min(n);
            let dropped = app.history.len() - shown;
            if app.history.cursor_index() < dropped {
                continue; // off-window: covered by its own test
            }
            let idx = app.history.cursor_index() - dropped;
            let slot = crate::history::slot_of_index(idx, shown, zoom, slots);
            let expected = gutter as u16 + (slot / spc) as u16;

            assert_eq!(
                cursor_column(&app, w, 12),
                Some(expected),
                "w={w} back={back}: readout displaced the marker"
            );
        }
    }
}

#[test]
fn scrubbing_past_the_left_edge_scrolls_the_window() {
    // G4 made the off-window case honest — an explicit marker and no figures.
    // G7 removes the case: Home now scrolls the graph to the oldest samples,
    // so the readout can state them because they are on screen.
    let mut app = App::new(600);
    for i in (0..500).rev() {
        app.push(sample_at(if i > 400 { 11.0 } else { 88.0 }, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.goto_oldest();

    let text = timeline_rows(&app, 100, 12).join("\n");
    assert!(
        text.contains("CPU 11.0%"),
        "window did not follow the cursor to the oldest sample:\n{text}"
    );
    assert!(
        !text.contains('◀'),
        "off-window marker shown when the window can reach the cursor"
    );
}

#[test]
fn the_live_view_does_not_shuffle_while_the_cursor_is_inside_it() {
    // Scrolling on every keypress would make the graph slide sideways under
    // the reader. The window only moves once the cursor would leave it.
    let mut app = App::new(600);
    for i in (0..500).rev() {
        app.push(sample_at((i as f32 * 1.7) % 100.0, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let graph = |a: &App| timeline_rows(a, 100, 12)[1..5].join("\n");
    let live = graph(&app);
    // A few steps back: still inside the live-anchored window.
    app.history.scrub(-3);
    assert_eq!(
        graph(&app),
        live,
        "graph moved while the cursor was still on it"
    );

    // Far enough back to leave it: now it must follow.
    app.history.scrub(-400);
    assert_ne!(graph(&app), live, "graph failed to follow the cursor");
}

#[test]
fn zoom_still_works_at_any_scroll_position() {
    let mut app = App::new(600);
    for i in (0..500).rev() {
        app.push(sample_at((i as f32 * 1.7) % 100.0, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.goto_oldest();

    let mut seen = std::collections::HashSet::new();
    for _ in 0..crate::app::ZOOM_LEVELS.len() {
        let rows = timeline_rows(&app, 100, 12);
        assert_eq!(rows.len(), 12);
        // The cursor must remain visible at every zoom level.
        assert!(
            rows.iter().any(|r| r.contains('▌') || r.contains('▐')),
            "cursor lost at zoom {}",
            app.zoom()
        );
        seen.insert(rows[1].clone());
        app.zoom_out();
    }
    assert!(seen.len() > 1, "zoom had no effect while scrolled back");
}

#[test]
fn the_readout_flips_side_rather_than_being_clipped() {
    // With the cursor at the newest sample the marker sits at the right edge,
    // so the text has to go to its left.
    let mut app = App::new(600);
    for i in (0..20).rev() {
        app.push(sample_at(77.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.scrub(-1);

    let rows = timeline_rows(&app, 100, 12);
    let cursor_row = rows
        .iter()
        .find(|r| r.contains('▌') || r.contains('▐'))
        .expect("cursor row");
    let marker = cursor_row.find(['▌', '▐']).unwrap();
    let text = cursor_row.find("CPU 77.0%").expect("readout missing");
    assert!(
        text < marker,
        "readout should sit left of a right-edge cursor: {cursor_row:?}"
    );
}

/// The distinct foreground colours used by the graph rows of the timeline,
/// excluding chrome (borders, gutter, threshold rules).
fn graph_colours(app: &App, w: u16, h: u16) -> std::collections::HashSet<String> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), app))
        .unwrap();
    let buf = term.backend().buffer();
    let graph_rows = (h as usize - 1).saturating_sub(2).max(1);
    let mut out = std::collections::HashSet::new();
    for row in 0..graph_rows {
        for x in 4..w {
            let c = &buf[(x, 1 + row as u16)];
            if c.fg != app.theme.chrome && c.symbol() != " " {
                out.insert(format!("{:?}", c.fg));
            }
        }
    }
    out
}

#[test]
fn timeline_colour_carries_identity_not_magnitude() {
    // Bar height already encodes the value. Colouring by the same number is
    // double-encoding: it spends the one free channel on information the chart
    // is already showing. An idle machine and a dying one must therefore draw
    // in the same hues, differing only in bar height.
    let build = |cpu: f32| {
        let mut app = App::new(600);
        for i in (0..60).rev() {
            let mut s = sample_at(cpu, i);
            s.mem.used = ((cpu / 100.0 * 16.0) as u64) << 30;
            app.push(s);
        }
        app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
        app
    };
    let idle = graph_colours(&build(5.0), 100, 14);
    let busy = graph_colours(&build(95.0), 100, 14);
    assert_eq!(
        idle, busy,
        "timeline recolours with magnitude; colour should mean which series"
    );
    assert!(
        !idle.is_empty(),
        "no graph colours found — test proves nothing"
    );
}

#[test]
fn the_two_series_are_told_apart_by_colour() {
    let mut app = App::new(600);
    for i in (0..60).rev() {
        app.push(sample_at(50.0, i));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    let seen = graph_colours(&app, 100, 14);
    assert!(
        seen.len() >= 2,
        "cpu and mem render in the same colour: {seen:?}"
    );
    assert!(seen.contains(&format!("{:?}", app.theme.series_cpu)));
    assert!(seen.contains(&format!("{:?}", app.theme.series_mem)));
}

#[test]
fn status_colour_is_kept_where_it_answers_is_this_bad() {
    // The header figures and the core meters are where a reader asks "is this
    // bad right now", not "what shape was this" — so heat earns its place
    // there and only there.
    let styles_at = |cpu: f32| {
        let mut app = App::new(60);
        app.push(sample(cpu));
        app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();
        // The header is rows 0-2: title, figures, then the core meters that
        // L2 folded in from their own section.
        let row = |y: u16| {
            (0..100u16)
                .map(|x| format!("{:?}", buf[(x, y)].fg))
                .collect::<String>()
        };
        (row(1), row(2))
    };
    let (h_idle, c_idle) = styles_at(5.0);
    let (h_busy, c_busy) = styles_at(95.0);
    assert_ne!(h_idle, h_busy, "header figures lost their status colour");
    assert_ne!(c_idle, c_busy, "core meters lost their status colour");
}

#[test]
fn status_and_identity_hues_stay_in_their_own_panels() {
    // The C6 rule, enforced against a rendered frame rather than the palette
    // definition — a palette-level test passes happily through a wiring bug,
    // which is exactly how G5 shipped `ok` green into the timeline.
    //
    // Both states are rendered. The timeline's cursor readout only exists while
    // scrubbed, and it prints the same figures the header colours with
    // `figure_style`, so it is the likeliest place for a status hue to leak
    // into the timeline — and a live-only render never draws it.
    for scrubbed in [false, true] {
        let mut app = App::new(600);
        for i in (0..80).rev() {
            // Sweep the range so every status band is actually reached.
            app.push(sample_at((i as f32 * 1.3) % 100.0, i));
        }
        app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
        if scrubbed {
            app.history.scrub(-6);
        }

        let (w, h) = (110u16, 30u16);
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();

        let status = [app.theme.ok, app.theme.warn, app.theme.critical];
        let identity = [app.theme.series_cpu, app.theme.series_mem];
        let timeline = ui::timeline_rows_range();

        let mut identity_marks = 0;
        let mut status_hues = std::collections::HashSet::new();
        for y in 0..h {
            for x in 0..w {
                let c = &buf[(x, y)];
                let blank = c.symbol() == " " || c.symbol() == "\u{2800}";
                if timeline.contains(&y) {
                    assert!(
                        !status.contains(&c.fg),
                        "scrubbed={scrubbed}: status hue {:?} in the timeline at ({x},{y})",
                        c.fg
                    );
                    // Count only drawn bars. Every cell in a graph row carries
                    // the series fg, blank ones included, so counting cells
                    // would still pass if the timeline drew nothing at all.
                    if identity.contains(&c.fg) && !blank {
                        identity_marks += 1;
                    }
                } else {
                    assert!(
                        !identity.contains(&c.fg),
                        "scrubbed={scrubbed}: identity hue {:?} outside the timeline at ({x},{y})",
                        c.fg
                    );
                    // Only count outside the header's LIVE badge: `live` is the
                    // `ok` hue by design, so it alone would satisfy a naive
                    // "some status colour appeared" check even with every
                    // figure, meter and table cell stripped of status colour.
                    if status.contains(&c.fg) && !blank && y >= ui::HEADER_H {
                        status_hues.insert(format!("{:?}", c.fg));
                    }
                }
            }
        }
        assert!(
            identity_marks > 20,
            "scrubbed={scrubbed}: timeline drew no data ({identity_marks} marks)"
        );
        assert!(
            !status_hues.is_empty(),
            "scrubbed={scrubbed}: no status colour outside the header at all"
        );
    }
}

#[test]
fn the_heat_ramp_states_its_scale() {
    // The 50/80 thresholds drove every colour decision in the UI and were
    // written down nowhere in it.
    let mut app = App::new(60);
    app.push(sample(50.0));
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let render = |w: u16| {
        let mut term = Terminal::new(TestBackend::new(w, 30)).unwrap();
        term.draw(|f| ui::draw(f, &app)).unwrap();
        let buf = term.backend().buffer();
        (0..30u16)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    };

    let wide = render(100);
    assert!(wide.contains("warn 50"), "heat ramp has no scale:\n{wide}");
    assert!(wide.contains("crit 80"));

    // A reference yields before the data it refers to.
    assert!(
        !render(30).contains("warn 50"),
        "scale did not yield when narrow"
    );
}

#[test]
fn the_scale_survives_a_host_with_no_per_core_data() {
    // It used to live on the cores panel, which is not drawn at all when the
    // platform reports no per-core figures — leaving the ramp that still
    // colours the header and the process table with no stated thresholds.
    let mut app = App::new(60);
    let mut s = sample(50.0);
    s.cpu_per_core.clear();
    app.push(s);
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    let text: String = (0..30u16)
        .flat_map(|y| (0..100u16).map(move |x| (x, y)))
        .map(|(x, y)| buf[(x, y)].symbol())
        .collect();
    assert!(
        !text.contains("cores ("),
        "fixture should have no cores panel"
    );
    assert!(
        text.contains("warn 50"),
        "scale vanished with the cores panel"
    );
}

#[test]
fn the_stated_scale_matches_the_colouring_it_describes() {
    // The previous version built its expectation from the same constants and
    // format literal as the code under test, so it pinned the title's shape
    // rather than its agreement with anything. This ties the printed numbers
    // to where `heat` actually changes colour.
    let th = Theme::new(Palette::Safe, Tier::TrueColor);
    assert_ne!(
        th.heat(Theme::WARN_PCT - 0.1),
        th.heat(Theme::WARN_PCT),
        "the printed warn threshold is not where the colour changes"
    );
    assert_ne!(
        th.heat(Theme::CRITICAL_PCT - 0.1),
        th.heat(Theme::CRITICAL_PCT),
        "the printed critical threshold is not where the colour changes"
    );

    let mut app = App::new(60);
    app.push(sample(50.0));
    app.theme = th;
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    let row: String = (0..100u16).map(|x| buf[(x, 0)].symbol()).collect();
    assert!(row.contains(&format!("warn {:.0}", Theme::WARN_PCT)));
    assert!(row.contains(&format!("crit {:.0}", Theme::CRITICAL_PCT)));
}

#[test]
fn the_graph_does_not_slide_on_a_single_keypress_while_scrolled_back() {
    // The bug this guards: deriving the window directly from the cursor drags
    // it one sample sideways on every keypress, so the graph slides under the
    // reader — and at zoom > 1 the buckets re-form and bar heights change too.
    // Paging keeps the window still until the cursor crosses a page boundary.
    let mut app = App::new(600);
    for i in (0..500).rev() {
        app.push(sample_at((i as f32 * 1.7) % 100.0, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    app.history.goto_oldest();

    let graph = |a: &App| timeline_rows(a, 100, 12)[1..5].join("\n");
    let before = graph(&app);
    app.history.scrub(1);
    assert_eq!(
        graph(&app),
        before,
        "graph slid sideways on a keypress that should only move the marker"
    );
}

#[test]
fn the_cursor_is_not_glued_to_the_left_edge_while_scrolled_back() {
    // Pinning the cursor to column 0 means never seeing anything older than
    // where you are — the very behaviour G7 exists to remove.
    let mut app = App::new(600);
    for i in (0..500).rev() {
        app.push(sample_at((i as f32 * 1.7) % 100.0, i as u64));
    }
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

    // Somewhere deep in the buffer, but not at its very start.
    app.history.goto_oldest();
    app.history.scrub(60);

    let x = cursor_column(&app, 100, 12).expect("cursor should be drawn");
    assert!(
        x > 6,
        "cursor is pinned near the left edge at column {x}; history older than \
         the cursor is unreachable"
    );
}

#[test]
fn every_scrub_position_keeps_the_cursor_inside_the_window() {
    // Paging must contain the cursor at every position and zoom, or the
    // off-window fallback becomes reachable again.
    for zoom_steps in 0..crate::app::ZOOM_LEVELS.len() {
        let mut app = App::new(600);
        for i in (0..500).rev() {
            app.push(sample_at(50.0, i as u64));
        }
        app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
        for _ in 0..zoom_steps {
            app.zoom_out();
        }
        app.history.goto_oldest();
        for step in 0..40 {
            assert!(
                cursor_column(&app, 100, 12).is_some(),
                "zoom step {zoom_steps}, scrub {step}: cursor left the window"
            );
            app.history.scrub(11);
        }
    }
}

#[test]
fn a_many_core_machine_summarises_rather_than_clipping() {
    // Cores that do not fit are counted, not dropped, so the number on screen
    // is never quietly wrong.
    //
    // The assertion is on the marker's *content*, not the row's length: every
    // TestBackend row is exactly `w` cells whatever was clipped, so a length
    // check is a tautology and passed the very bug this now catches — at 13
    // columns a 16-core machine rendered `16 cores  +1`, the marker sized from
    // the core count and then clipped by ratatui.
    for cores in [16usize, 128, 1024] {
        let mut app = App::new(60);
        let mut s = sample(50.0);
        s.cpu_per_core = (0..cores).map(|i| (i as f32 * 0.78) % 100.0).collect();
        app.push(s);
        app.theme = Theme::new(Palette::Safe, Tier::TrueColor);

        for w in [13u16, 16, 24, 80, 100, 200] {
            let mut term = Terminal::new(TestBackend::new(w, 30)).unwrap();
            term.draw(|f| ui::draw(f, &app)).unwrap();
            let buf = term.backend().buffer();
            let row: String = (0..w).map(|x| buf[(x, 2)].symbol()).collect();
            let drawn = row.chars().filter(|c| BAR_GLYPHS.contains(c)).count();
            // Whatever it degrades to, the count itself is always stated.
            assert!(
                row.contains(&cores.to_string()),
                "cores={cores} w={w}: core count missing from {row:?}"
            );

            match row.split_once('+') {
                Some((_, tail)) => {
                    let hidden: usize = tail.trim().parse().unwrap_or_else(|_| {
                        panic!("cores={cores} w={w}: unreadable marker {row:?}")
                    });
                    assert_eq!(
                        drawn + hidden,
                        cores,
                        "cores={cores} w={w}: {drawn} drawn + {hidden} hidden != {cores}, in {row:?}"
                    );
                }
                // No marker is honest in exactly two cases: everything is
                // drawn, or nothing is and the line states the count alone.
                // A partial draw with no marker is the silent lie.
                None => assert!(
                    drawn == cores || drawn == 0,
                    "cores={cores} w={w}: {drawn} of {cores} drawn with no marker, in {row:?}"
                ),
            }
        }
    }
}

/// The eighth-block glyphs the meters draw with.
const BAR_GLYPHS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

#[test]
fn a_host_with_no_per_core_data_says_so() {
    // The line is part of the header now, so it is always drawn — silence
    // would read as "zero cores" rather than "not reported".
    let mut app = App::new(60);
    let mut s = sample(50.0);
    s.cpu_per_core.clear();
    app.push(s);
    app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| ui::draw(f, &app)).unwrap();
    let buf = term.backend().buffer();
    let row: String = (0..100u16).map(|x| buf[(x, 2)].symbol()).collect();
    assert!(
        row.contains("not reported"),
        "silent about missing cores: {row:?}"
    );
}

#[test]
#[ignore = "visual"]
fn show_core_overflow() {
    for cores in [16usize, 128, 1024] {
        for w in [13u16, 16, 24, 40, 80] {
            let mut app = App::new(60);
            let mut s = sample(50.0);
            s.cpu_per_core = (0..cores).map(|i| (i as f32 * 0.78) % 100.0).collect();
            app.push(s);
            app.theme = Theme::new(Palette::Safe, Tier::TrueColor);
            let mut term = Terminal::new(TestBackend::new(w, 30)).unwrap();
            term.draw(|f| ui::draw(f, &app)).unwrap();
            let buf = term.backend().buffer();
            let row: String = (0..w).map(|x| buf[(x, 2)].symbol()).collect();
            println!("  {cores:>4} cores, w={w:<4} |{}|", row);
        }
    }
}
