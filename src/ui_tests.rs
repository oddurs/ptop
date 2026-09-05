//! Headless render tests.
//!
//! A TUI that panics on a 20-column terminal or an empty buffer is worse than
//! no TUI, and neither case shows up in normal use. `TestBackend` renders into
//! a plain buffer so both are cheap to exercise.

use crate::app::App;
use crate::sample::{MemStat, ProcSample, Sample};
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
    let mut app = App::new(600);
    for i in (0..200).rev() {
        app.push(sample_at(50.0, i));
    }
    let mut term = Terminal::new(TestBackend::new(60, 10)).unwrap();
    term.draw(|f| ui::draw_timeline_for_test(f, f.area(), &app))
        .unwrap();
    let buf = term.backend().buffer();
    // Rows 1..=8 are inside the border; none may be entirely empty.
    for y in 1..9 {
        let row: String = (0..60).map(|x| buf[(x, y)].symbol()).collect();
        let inner = &row[row.char_indices().nth(1).unwrap().0..];
        assert!(
            inner.chars().any(|c| c != ' ' && c != '│'),
            "row {y} is blank: {row:?}"
        );
    }
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
