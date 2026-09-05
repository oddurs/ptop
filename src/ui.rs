//! Rendering.
//!
//! Layout, top to bottom: a summary header, per-core meters, the timeline
//! (the reason this tool exists), the process table, and a help line.

use crate::app::{self, App};
use crate::glyphs::{self, GlyphSet};
use crate::history;
use crate::sample::{IoRates, Sample};
use crate::theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Table};
use std::time::Duration;

/// Eighth-block glyphs, used to draw the timeline one cell per sample.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Panel heights. Named so tests can locate a panel from the same numbers the
/// renderer lays out with, rather than re-deriving them from a comment that
/// silently goes stale.
// One divider row per section instead of two border rows per panel, and no
// side borders at all, which returned five rows and two columns to the data.
// Boxes also compete with their own contents for attention; a hairline rule
// separates without doing that. L2 then folded the cores section in here,
// trading its divider and its data row for one header line.
pub const HEADER_H: u16 = 3; // title, figures, cores
/// The height the timeline had when it was fixed.
///
/// It is the floor, not a minimum for legibility: growing a panel must never
/// shrink it. A proportional height alone gave an 80x24 terminal five rows
/// where nine were fixed before — a quarter of the CPU resolution, on the most
/// common terminal size, from a change justified by *more* resolution.
pub const TIMELINE_MIN_H: u16 = 9;
/// Beyond this, rows stop buying resolution and start starving the table.
pub const TIMELINE_MAX_H: u16 = 16;
/// Rows reserved for the process table when deciding the timeline's height.
///
/// A reservation, not a layout constraint: the table is laid out with whatever
/// remains, so this number and the layout cannot drift apart.
pub const PROCS_RESERVE_H: u16 = 6;
/// The table always keeps at least this much, however cramped the terminal.
const PROCS_FLOOR_H: u16 = 2;

/// Timeline height for a terminal of `total` rows.
///
/// Proportional above the floor, because rows are resolution here: each braille
/// row carries four levels, so the eleven-row panel a 38-row terminal gives has
/// twenty distinct CPU heights against the twelve of the old fixed nine.
///
/// Never below the height it used to have, and never so tall the process table
/// cannot be read.
pub fn timeline_height(total: u16) -> u16 {
    let spare = total.saturating_sub(HEADER_H + PROCS_RESERVE_H + 1);
    let want = (spare * 2 / 5).clamp(TIMELINE_MIN_H, TIMELINE_MAX_H);
    // On a terminal too small for the floor, take what is left over — but never
    // everything, or the table disappears.
    want.min(total.saturating_sub(HEADER_H + 1 + PROCS_FLOOR_H).max(1))
}

/// Rows occupied by the timeline panel.
///
/// Test-only: it exists so a test can locate the panel from the same function
/// the renderer lays out with. Accurate because the timeline is a `Length` and
/// the table takes what remains — with a `Min` on the table, ratatui would
/// outrank the timeline and this would report a panel that is not there.
#[cfg(test)]
pub fn timeline_rows_range(total_height: u16) -> std::ops::Range<u16> {
    let top = HEADER_H;
    top..top + timeline_height(total_height)
}

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(HEADER_H),
        Constraint::Length(timeline_height(f.area().height)),
        // Whatever remains. `timeline_height` has already reserved the table's
        // share, and a `Min` here would outrank the timeline's `Length` and
        // silently shrink it below the height that function reports.
        Constraint::Min(1),
        Constraint::Length(1), // help
    ])
    .split(f.area());

    let Some(sample) = app.history.current() else {
        f.render_widget(
            Paragraph::new("collecting first sample…").style(app.theme.dim_style()),
            f.area(),
        );
        return;
    };

    draw_header(f, chunks[0], app, sample);
    draw_timeline(f, chunks[1], app);
    draw_procs(f, chunks[2], app);
    draw_help(f, chunks[3], app);
}

/// A section rule with its name on it, replacing a panel border.
///
/// The rule takes the most recessive token and the name a readable but still
/// recessive one, so neither competes with the figures beneath.
fn divider(title: &str, width: u16, theme: &Theme) -> Line<'static> {
    let name = format!(" {} ", title.trim());
    let lead = "─".repeat(2.min(width as usize));
    let used = lead.chars().count() + name.chars().count();
    let tail = "─".repeat((width as usize).saturating_sub(used));
    Line::from(vec![
        Span::styled(lead, theme.chrome_style()),
        Span::styled(name, theme.title_style()),
        Span::styled(tail, theme.chrome_style()),
    ])
}

fn fmt_bytes(b: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{v:.0}{}", UNITS[i])
    } else {
        format!("{v:.1}{}", UNITS[i])
    }
}

/// Compact lag for the paused badge: seconds under a minute, then m/s.
fn fmt_lag(d: Duration) -> String {
    let s = d.as_secs();
    if s < 60 {
        format!("{s}s")
    } else {
        format!("{}m{:02}s", s / 60, s % 60)
    }
}

fn fmt_uptime(d: Duration) -> String {
    let s = d.as_secs();
    let (days, hours, mins) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60);
    if days > 0 {
        format!("{days}d {hours}h {mins}m")
    } else {
        format!("{hours}h {mins}m")
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App, s: &Sample) {
    let mem_pct = s.mem.used_pct();
    let mut spans = vec![
        Span::styled("CPU ", app.theme.dim_style()),
        Span::styled(
            format!("{:>5.1}%", s.cpu_total),
            app.theme.figure_style(s.cpu_total),
        ),
        Span::raw("   "),
        Span::styled("MEM ", app.theme.dim_style()),
        Span::styled(
            format!("{:>5.1}%", mem_pct),
            app.theme.figure_style(mem_pct),
        ),
        Span::styled(
            format!(
                " ({} / {}, {} avail)",
                fmt_bytes(s.mem.used),
                fmt_bytes(s.mem.total),
                fmt_bytes(s.mem.available)
            ),
            app.theme.dim_style(),
        ),
    ];

    if s.mem.swap_total > 0 {
        spans.push(Span::styled("   SWP ", app.theme.dim_style()));
        spans.push(Span::styled(
            format!("{:>5.1}%", s.mem.swap_pct()),
            app.theme.heat_style(s.mem.swap_pct()),
        ));
    }

    spans.extend([
        Span::styled("   LOAD ", app.theme.dim_style()),
        Span::raw(format!(
            "{:.2} {:.2} {:.2}",
            s.load[0], s.load[1], s.load[2]
        )),
        Span::styled("   UP ", app.theme.dim_style()),
        Span::raw(fmt_uptime(s.uptime)),
        Span::styled("   PROCS ", app.theme.dim_style()),
        Span::raw(s.procs.len().to_string()),
    ]);

    let state = if app.history.is_live() {
        Span::styled(" ptop — LIVE ", app.theme.live_style())
    } else {
        // Loud on purpose: reading a stale process table as the current one is
        // the single worst thing this tool could let you do.
        Span::styled(
            format!(" ptop — PAUSED  -{} ", fmt_lag(app.history.time_behind())),
            app.theme.paused_style(),
        )
    };

    // The heat scale lives on the title line: it is a reference for the
    // figures just below it, and the header is always drawn.
    let mut title = vec![state];
    if let Some(scale) = heat_scale(area.width) {
        title.push(Span::styled(scale, app.theme.dim_style()));
    }
    let title = Line::from(title);

    f.render_widget(
        Paragraph::new(vec![
            title,
            Line::from(spans),
            core_meters(s, area.width, &app.theme),
        ]),
        area,
    );
}

/// The heat ramp's scale, when the panel is wide enough to carry it.
///
/// Semantic heat is a legitimate multi-hue ramp, but only with its scale
/// stated. After G5 the ramp survives in three places — the header figures,
/// the core meters and the process table's CPU column — and the numbers behind
/// the colour change appeared nowhere in the UI. They are the same thresholds
/// the timeline draws rules at, so saying them once is enough.
///
/// Measured in columns, not bytes: `·` is two bytes and one column, so byte
/// length silently over-reserves, and would be three times wrong if the text
/// ever gained a wide character.
fn heat_scale(width: u16) -> Option<String> {
    let scale = format!(
        "· warn {:.0} · crit {:.0} ",
        Theme::WARN_PCT,
        Theme::CRITICAL_PCT
    );
    // Room for the state badge. There are no border columns to reserve since
    // L1 — the title is a full-width content line now.
    const RESERVED: usize = 24;
    (width as usize >= scale.chars().count() + RESERVED).then_some(scale)
}

/// The per-core meters, as one line of the header.
///
/// They were a section of their own: two rows, of which one was a divider and
/// one was a single line of glyphs. That is a section's worth of chrome for a
/// status strip, and it read as a chart when it is really a row of figures.
///
/// One column per core rather than two. A 128-core machine needs 128 columns
/// at one glyph each, which a wide terminal has; at two it needs 256, which
/// nothing has. Cores beyond the width are summarised rather than clipped, so
/// the count is never silently wrong.
fn core_meters(s: &Sample, width: u16, theme: &Theme) -> Line<'static> {
    if s.cpu_per_core.is_empty() {
        return Line::from(Span::styled("cores: not reported", theme.dim_style()));
    }
    let n = s.cpu_per_core.len();
    let w = width as usize;
    let label = format!("{n:>3} cores ");

    // Too narrow even for the label: state the count and draw nothing. A row of
    // meters that cannot say how many are missing is worse than no meters.
    if label.chars().count() >= w {
        return Line::from(Span::styled(format!("{n} cores"), theme.dim_style()));
    }
    let avail = w - label.chars().count();

    // The marker's width depends on how many are hidden, which depends on how
    // many fit — so shrink until the whole line fits rather than reserving a
    // guess. Reserving `" +{n}"` while printing `" +{overflow}"` wasted a
    // column, and once the reservation exceeded the room available it produced
    // a marker ratatui then clipped: `16 cores  +1` for sixteen hidden cores.
    let marker_len = |shown: usize| {
        if shown == n {
            0
        } else {
            format!(" +{}", n - shown).chars().count()
        }
    };
    let mut shown = n.min(avail);
    while shown + marker_len(shown) > avail {
        if shown == 0 {
            break;
        }
        shown -= 1;
    }
    // Even a bare marker does not fit. State the count alone: drawing meters
    // that cannot say how many are missing is the failure this exists to avoid.
    if shown + marker_len(shown) > avail {
        return Line::from(Span::styled(format!("{n} cores"), theme.dim_style()));
    }

    let mut spans = vec![Span::styled(label, theme.dim_style())];
    spans.extend(s.cpu_per_core.iter().take(shown).map(|&pct| {
        let idx = ((pct / 100.0 * 7.0).round() as usize).min(7);
        Span::styled(BARS[idx].to_string(), theme.heat_style(pct))
    }));
    if shown < n {
        spans.push(Span::styled(format!(" +{}", n - shown), theme.dim_style()));
    }
    Line::from(spans)
}

/// Draw just the timeline panel, for visual inspection in tests.
#[cfg(test)]
pub fn draw_timeline_for_test(f: &mut Frame, area: Rect, app: &App) {
    draw_timeline(f, area, app);
}

/// The scrubable timeline, oldest on the left.
///
/// Two packings compose here: each character cell holds `samples_per_cell`
/// display slots, and each slot aggregates `zoom` samples by peak. At zoom 5
/// with braille that is ten seconds per cell, so a normal terminal shows the
/// entire buffer.
fn draw_timeline(f: &mut Frame, area: Rect, app: &App) {
    let inner_w = area.width as usize;
    let inner_h = area.height.saturating_sub(1) as usize;
    if inner_w == 0 || inner_h == 0 {
        return;
    }

    // Reserve the cursor marker and legend, then divide the rest exactly so no
    // row is left blank. CPU takes the larger share as the spikier signal.
    let graph_rows = inner_h.saturating_sub(2).max(1);
    let cpu_rows = (graph_rows * 3 / 5).max(1);
    let mem_rows = graph_rows.saturating_sub(cpu_rows);

    // A left gutter carrying the scale. Dropped entirely on a narrow panel:
    // four columns of axis is a poor trade against four columns of history
    // when there is little room, and the threshold rules still anchor the
    // graph without it.
    // Reserve the gutter only when some section can actually fill it. A section
    // shorter than `MIN_ROWS_FOR_AXIS` carries no anchors and no label, so the
    // columns would be blank while the graph lost that much history.
    let graph_rows_probe = inner_h.saturating_sub(2).max(1);
    let cpu_probe = (graph_rows_probe * 3 / 5).max(1);
    let mem_probe = graph_rows_probe.saturating_sub(cpu_probe);
    let gutter = if inner_w >= MIN_WIDTH_FOR_GUTTER
        && (cpu_probe >= MIN_ROWS_FOR_AXIS || mem_probe >= MIN_ROWS_FOR_AXIS)
    {
        GUTTER_W
    } else {
        0
    };
    let graph_w = inner_w.saturating_sub(gutter);

    let spc = app.glyphs.samples_per_cell();
    let slots = graph_w * spc;
    let samples: Vec<&Sample> = app.history.iter().collect();
    let zoom = app::effective_zoom(app.zoom(), samples.len(), slots);
    let shown = (slots * zoom).min(samples.len());
    // Text-editor scrolling. The window stays anchored to the live edge while
    // the cursor is inside it, and follows only once the cursor would leave —
    // so the live view never shuffles, and scrubbing never takes you somewhere
    // you cannot see.
    //
    // Stateless on purpose: the window position is derived from the cursor each
    // frame rather than stored, so there is no scroll offset to keep in sync
    // with a buffer that is being written to at the same time.
    let window_start = window_start(&app.history, shown);
    let window = &samples[window_start..window_start + shown];

    let cpu: Vec<f32> = window.iter().map(|s| s.cpu_total).collect();
    let mem: Vec<f32> = window.iter().map(|s| s.mem.used_pct()).collect();
    let cpu_slots = history::peak_slots(&cpu, zoom, slots);
    let mem_slots = history::peak_slots(&mem, zoom, slots);

    // The threshold rule. Its whole point is that the boundary is readable
    // without colour — until now the 50/80 thresholds existed *only* as a hue
    // change, which is invisible to the most common colour vision deficiency
    // and to anyone on a monochrome terminal. It doubles as the scale anchor
    // the graph otherwise completely lacked.
    // Whether the gutter can name each series itself. A direct label beats a
    // legend — the reader stops having to hold "top is cpu" in their head — but
    // it needs a row that is not already carrying an axis anchor.
    let labelled = gutter > 0 && cpu_rows >= MIN_ROWS_FOR_LABEL && mem_rows >= MIN_ROWS_FOR_LABEL;

    let mut lines: Vec<Line> = Vec::with_capacity(inner_h);
    for (values, rows, name, series) in [
        (&cpu_slots, cpu_rows, "CPU", app.theme.series_cpu),
        (&mem_slots, mem_rows, "MEM", app.theme.series_mem),
    ] {
        // Both thresholds, not just critical. The warn boundary is the one the
        // roadmap actually asked for, and leaving it hue-only kept it invisible
        // to the commonest colour vision deficiency and on any mono terminal.
        let rules: Vec<(usize, usize)> = [Theme::WARN_PCT, Theme::CRITICAL_PCT]
            .iter()
            .filter_map(|&pct| glyphs::rule_position(pct, rows))
            .collect();
        for row in 0..rows {
            let rule_level = rules.iter().find(|(r, _)| *r == row).map(|(_, l)| *l);
            let mut spans = axis_label(row, rows, gutter, &app.theme, labelled.then_some(name));
            spans.extend(
                glyph_row(
                    GraphRow {
                        set: app.glyphs,
                        values,
                        row,
                        rows,
                        spc,
                        rule_level,
                        series,
                    },
                    &app.theme,
                )
                .spans,
            );
            lines.push(Line::from(spans));
        }
    }

    lines.push(cursor_row(
        app,
        Window {
            len: window.len(),
            start: window_start,
            zoom,
            slots,
            spc,
            graph_w,
            gutter,
        },
    ));

    if lines.len() < inner_h {
        let span = fmt_lag(Duration::from_secs(window.len() as u64));
        // Only the identification half is dropped when the gutter names the
        // series. The span, the slot size and the keys are not a legend and
        // are not duplicated anywhere else.
        let ident = if labelled { "" } else { "cpu · mem — " };
        lines.push(Line::from(Span::styled(
            format!("{ident}{span} shown, {zoom}s/slot — ←/→ scrub, +/- zoom"),
            app.theme.dim_style(),
        )));
    }

    let title = format!(
        " timeline — {} of {} buffered ",
        fmt_lag(Duration::from_secs(app.history.len() as u64)),
        fmt_lag(Duration::from_secs(app.history.capacity() as u64)),
    );

    let mut all = vec![divider(&title, area.width, &app.theme)];
    all.extend(lines);
    f.render_widget(Paragraph::new(all), area);
}

/// One row of graph. `row` counts from the top of a `rows`-tall graph.
/// Everything one graph row needs to draw itself. Bundled because seven
/// positional parameters had become eight and the call site was unreadable.
/// `row` counts from the top of a `rows`-tall graph.
struct GraphRow<'a> {
    set: GlyphSet,
    values: &'a [Option<f32>],
    /// Counted from the top of a `rows`-tall graph.
    row: usize,
    rows: usize,
    /// Samples per character cell.
    spc: usize,
    /// Dot height of a threshold rule crossing this row, if any.
    rule_level: Option<usize>,
    /// Identity of the series — never a judgement about its value.
    series: Color,
}

/// Draw one row of a graph.
fn glyph_row(g: GraphRow, theme: &Theme) -> Line<'static> {
    let (set, values, row, rows, spc, rule_level, series) = (
        g.set,
        g.values,
        g.row,
        g.rows,
        g.spc,
        g.rule_level,
        g.series,
    );
    let spans = values
        .chunks(spc)
        .enumerate()
        .map(|(i, cell)| {
            let pcts: Vec<f32> = cell.iter().map(|v| v.unwrap_or(0.0)).collect();
            let left = glyphs::level_in_row(pcts[0], row, rows);
            let right = glyphs::level_in_row(*pcts.get(1).unwrap_or(&pcts[0]), row, rows);
            // Colour is identity here, not magnitude — see `Theme::series_style`.
            // The threshold rules now carry "is this bad", which is what the
            // heat ramp was doing redundantly on top of the bar height.
            // Data always wins the cell. The rule fills gaps only, and dashes
            // so it reads as a reference line rather than a row of samples — at
            // the mono tier a solid rule is indistinguishable from a low bar,
            // since both render a dim `⣀`.
            let empty = left.max(right) == 0;
            match rule_level {
                Some(lvl) if empty && i % 2 == 0 => {
                    Span::styled(set.rule_glyph(lvl).to_string(), theme.chrome_style())
                }
                _ => Span::styled(
                    set.glyph(left, right).to_string(),
                    theme.series_style(series),
                ),
            }
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// Width of the scale gutter, and the panel width below which it is dropped.
const GUTTER_W: usize = 4;
const MIN_WIDTH_FOR_GUTTER: usize = 30;
/// A section shorter than this cannot carry both ends of the scale, so it
/// carries none: see [`axis_label`].
const MIN_ROWS_FOR_AXIS: usize = 2;
/// A section needs a row spare — beyond the two anchors — to name itself.
const MIN_ROWS_FOR_LABEL: usize = 3;

// The gutter must fit inside the panel it is dropped from, or `graph_w`
// underflows. The two constants are unrelated by construction, so tie them.
const _: () = assert!(MIN_WIDTH_FOR_GUTTER > GUTTER_W);

/// The scale marks for one graph row: `100` on the top row, `0` on the bottom.
///
/// A percentage graph always spans 0..100, so these do not tell a reader
/// anything they could not assume — what they do is anchor the *geometry*, so
/// a bar's height can be read as a value rather than only compared to its
/// neighbours.
fn axis_label(
    row: usize,
    rows: usize,
    gutter: usize,
    theme: &Theme,
    series: Option<&str>,
) -> Vec<Span<'static>> {
    if gutter == 0 {
        return Vec::new();
    }
    // The caller decides whether a section is tall enough to name itself. Tie
    // that decision to this function, or a second caller could pass a label
    // into a two-row section and watch it silently vanish.
    debug_assert!(
        series.is_none() || rows >= MIN_ROWS_FOR_LABEL,
        "a label was passed to a section with only {rows} rows"
    );
    // A section only one row tall spans the entire 0..100 range in that row.
    // Labelling its top `100` states that the top of the row is the maximum,
    // which is true, while implying the bottom is not the minimum — and `0`
    // never appears at all. An axis that misleads is worse than no axis, so a
    // section too short to carry both ends carries neither.
    let text = if rows < MIN_ROWS_FOR_AXIS {
        String::new()
    } else if row == 0 {
        "100".to_string()
    } else if row + 1 == rows {
        "0".to_string()
    } else if row == 1 {
        // The first row not already carrying an anchor.
        series.unwrap_or_default().to_string()
    } else {
        String::new()
    };
    // Right-aligned in `gutter - 1`, leaving one column of separation, so the
    // label stays exactly `gutter` wide whatever the constant becomes.
    let w = gutter.saturating_sub(1);
    vec![Span::styled(
        format!("{text:>w$} ", text = text, w = w),
        theme.dim_style(),
    )]
}

/// Index of the first sample drawn.
///
/// The buffer is divided into pages of `shown` samples counted back from the
/// live edge, and the window shows whichever page the cursor is on. Page 0 is
/// the live window, so the live view is unchanged.
///
/// Paging rather than following. A window derived directly from the cursor
/// drags one sample sideways on every keypress — the graph slides under the
/// reader, and at `zoom > 1` the buckets re-form so bar heights change too. It
/// also pins the cursor to column 0, which means never seeing anything *older*
/// than where you are: exactly the behaviour G7 exists to remove, reintroduced.
///
/// Paging gives that hysteresis without storing a scroll offset. `ui::draw`
/// takes `&App`, and the window size depends on panel width and zoom — both
/// render-time facts — so a remembered offset would need interior mutability
/// or a mutable draw. Deriving the page from the cursor needs neither.
///
/// `peak_slots` remains right-aligned within whatever slice it is given; only
/// the choice of slice changed. The right edge is therefore "newest in the
/// window" rather than "now", which is why the axis claims `now` only while
/// live and shows the cursor's own figures otherwise.
fn window_start(history: &history::History, shown: usize) -> usize {
    let len = history.len();
    let last_page = len.saturating_sub(shown);
    if history.is_live() || shown == 0 {
        return last_page;
    }
    // Pages counted back from the live edge, so page 0 is the live window.
    let from_newest = len.saturating_sub(1) - history.cursor_index();
    let page = from_newest / shown;
    last_page.saturating_sub(page * shown)
}

/// Where the timeline's window sits and how it maps to columns. Bundled for
/// the same reason `GraphRow` is: the parameter list had outgrown readability.
struct Window {
    /// Samples drawn.
    len: usize,
    /// Index of the first sample drawn.
    start: usize,
    zoom: usize,
    slots: usize,
    /// Samples per character cell.
    spc: usize,
    /// Columns available to the graph, excluding the gutter.
    graph_w: usize,
    gutter: usize,
}

/// The row under the graph marking where the scrub cursor sits.
///
/// When a cell holds two samples the marker picks the correct half, so packing
/// never costs cursor precision.
fn cursor_row(app: &App, w: Window) -> Line<'static> {
    let (n_values, window_start, zoom, slots, spc, graph_w, gutter) =
        (w.len, w.start, w.zoom, w.slots, w.spc, w.graph_w, w.gutter);
    let pad = " ".repeat(gutter);
    if app.history.is_live() || n_values == 0 {
        return Line::from(Span::styled(
            format!(
                "{pad}{:<width$}now",
                "past",
                width = graph_w.saturating_sub(3)
            ),
            app.theme.dim_style(),
        ));
    }

    // The cursor is an index into the whole buffer; the graph shows a window of
    // the newest `n_values`, so rebase before locating it.
    // Paging always contains the cursor, so this is unreachable. Asserted in
    // debug so a windowing mistake is loud in tests, but kept as a fallback in
    // release: a wrong marker is a bug, a panicking monitor is worse.
    debug_assert!(
        app.history.cursor_index() >= window_start,
        "cursor {} precedes window start {window_start}",
        app.history.cursor_index()
    );
    if app.history.cursor_index() < window_start {
        let mut row = vec![' '; graph_w];
        if let Some(c) = row.first_mut() {
            *c = '◀';
        }
        return Line::from(vec![
            Span::raw(pad),
            Span::styled(
                row.into_iter().collect::<String>(),
                app.theme.cursor_style(),
            ),
        ]);
    }

    let idx = app.history.cursor_index() - window_start;
    let slot = history::slot_of_index(idx, n_values, zoom, slots);

    let cell = (slot / spc).min(graph_w.saturating_sub(1));
    let marker = app.glyphs.cursor_marker(spc == 2 && slot % spc == 1);

    // The values at the cursor, beside the cursor. A terminal has no hover, so
    // the scrub marker *is* the crosshair — and its readout has been living in
    // the header, far from where the eye is actually fixed.
    let readout = app
        .history
        .current()
        // One decimal, matching the header: both are on screen while
        // scrubbing, so rounding them differently makes a sample at 89.6% read
        // `89.6` in one place and `90` in the other.
        .map(|s| format!("CPU {:.1}%  MEM {:.1}%", s.cpu_total, s.mem.used_pct()));

    // Prefer to the right of the marker; fall back to the left when the cursor
    // is near the right edge, so the text can never overflow the panel.
    let mut spans = vec![Span::raw(pad)];
    let right_room = graph_w.saturating_sub(cell + 1);
    let left_room = cell;
    match readout {
        Some(text) if right_room > text.chars().count() => {
            spans.push(Span::raw(" ".repeat(cell)));
            spans.push(Span::styled(marker.to_string(), app.theme.cursor_style()));
            spans.push(Span::styled(format!(" {text}"), app.theme.dim_style()));
            let used = cell + 1 + 1 + text.chars().count();
            spans.push(Span::raw(" ".repeat(graph_w - used)));
        }
        Some(text) if left_room > text.chars().count() => {
            let lead = cell - text.chars().count() - 1;
            spans.push(Span::raw(" ".repeat(lead)));
            spans.push(Span::styled(format!("{text} "), app.theme.dim_style()));
            spans.push(Span::styled(marker.to_string(), app.theme.cursor_style()));
            spans.push(Span::raw(" ".repeat(right_room)));
        }
        // No room either side: the marker alone still locates the sample, and
        // the header still carries the figures.
        _ => {
            spans.push(Span::raw(" ".repeat(cell)));
            spans.push(Span::styled(marker.to_string(), app.theme.cursor_style()));
            spans.push(Span::raw(" ".repeat(right_room)));
        }
    }
    Line::from(spans)
}

fn draw_procs(f: &mut Frame, area: Rect, app: &App) {
    let rows_data = app.visible_rows();
    let collected = app.history.current().is_some_and(|s| s.io_collected);
    let rows_visible = area.height.saturating_sub(2) as usize;

    // Keep the selected row on screen while scrolling through a long list.
    let offset = app.selected.saturating_sub(rows_visible.saturating_sub(1));

    let rows: Vec<Row> = rows_data
        .iter()
        .enumerate()
        .skip(offset)
        .take(rows_visible)
        .map(|(i, r)| {
            let p = r.proc;
            let mut style = Style::default();
            if i == app.selected {
                style = app.theme.selection_style();
            } else if r.context_only {
                // Present only as an ancestor of a filter match: visible for
                // parentage, but clearly not itself a hit.
                style = style.add_modifier(Modifier::DIM);
            }
            let mut cells = vec![
                Cell::from(p.pid.to_string()),
                Cell::from(p.user.to_string()),
                Cell::from(format!("{:.1}", p.cpu)).style(app.theme.heat_style(p.cpu)),
                Cell::from(fmt_bytes(p.rss)),
                Cell::from(p.state.to_string()),
                Cell::from(p.threads.to_string()),
            ];
            if app.show_io {
                cells.push(io_cell(collected, p.io, false, &app.theme));
                cells.push(io_cell(collected, p.io, true, &app.theme));
            }
            // The spine is structural, not data: it takes the chrome token so
            // it recedes the way a gridline should, while the name stays at
            // full contrast.
            cells.push(Cell::from(Line::from(vec![
                Span::styled(r.prefix.clone(), app.theme.chrome_style()),
                Span::raw(p.name.to_string()),
            ])));
            Row::new(cells).style(style)
        })
        .collect();

    let mut header_cells = vec!["PID", "USER", "CPU%", "RSS", "S", "THR"];
    if app.show_io {
        header_cells.extend(["DISK R", "DISK W"]);
    }
    header_cells.push("COMMAND");
    let header = Row::new(header_cells).style(app.theme.table_header_style());

    let title = format!(
        " processes ({}) — sort: {}{}{} ",
        rows_data.len(),
        app.sort.label(),
        if app.tree { " · tree" } else { "" },
        io_status(app, collected, &rows_data)
    );

    let mut widths = vec![
        Constraint::Length(7),
        Constraint::Length(10),
        Constraint::Length(6),
        Constraint::Length(8),
        Constraint::Length(2),
        Constraint::Length(4),
    ];
    if app.show_io {
        widths.push(Constraint::Length(9));
        widths.push(Constraint::Length(9));
    }
    widths.push(Constraint::Min(10));

    f.render_widget(
        Paragraph::new(divider(&title, area.width, &app.theme)),
        Rect { height: 1, ..area },
    );
    let table = Table::new(rows, widths).header(header);
    f.render_widget(
        table,
        Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        },
    );
}

/// One disk-rate cell.
///
/// Three distinct states, none of them a zero: `·` for history recorded before
/// the column was switched on, `—` for a process this user may not read, and a
/// rate otherwise.
fn io_cell(collected: bool, io: Option<IoRates>, write: bool, theme: &Theme) -> Cell<'static> {
    let dim = theme.dim_style();
    match (collected, io) {
        (false, _) => Cell::from("·").style(dim),
        (true, None) => Cell::from("—").style(dim),
        (true, Some(io)) => {
            let bytes = if write { io.write } else { io.read };
            if bytes == 0 {
                Cell::from("0").style(dim)
            } else {
                Cell::from(format!("{}/s", fmt_bytes(bytes)))
            }
        }
    }
}

/// Panel-title note about IO availability.
///
/// If most processes are unreadable the table would otherwise look broken; this
/// says why, and implies the fix.
fn io_status(app: &App, collected: bool, rows: &[crate::tree::TreeRow]) -> String {
    if !app.show_io {
        return String::new();
    }
    if !collected {
        return " · io: not collected here".into();
    }
    // Only unreadable processes are worth mentioning: a process awaiting its
    // second reading also shows a dash, but resolves on its own and needs no
    // action from anyone.
    let denied = app.history.current().map_or(0, |s| s.io_denied);
    if denied == 0 {
        " · io".into()
    } else {
        format!(" · io: {denied}/{} need root", rows.len())
    }
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let line = if app.editing_filter {
        Line::from(vec![
            Span::styled("filter: ", app.theme.cursor_style()),
            Span::raw(&app.filter),
            Span::styled("█", app.theme.cursor_style()),
            Span::styled("   (Enter/Esc to finish)", app.theme.dim_style()),
        ])
    } else {
        Line::from(Span::styled(
            "q quit · ←/→ scrub · +/- zoom · Space live · ↑/↓ select · s sort · t tree · i io · / filter",
            app.theme.dim_style(),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}
