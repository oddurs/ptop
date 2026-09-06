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
/// Exposed for tests: the format is a claim about legibility at both ends of
/// the configurable range, which is only checkable by reading the string.
#[cfg(test)]
pub fn fmt_lag_for_test(d: Duration) -> String {
    fmt_lag(d)
}

fn fmt_lag(d: Duration) -> String {
    // Both ends matter now that the interval is configurable. Whole seconds
    // rendered a 100ms slot as `0s/slot` — the exact mode sub-second sampling
    // exists for — and a day-long window as `1440m00s buffered`, which is a
    // number nobody can read as a day.
    let ms = d.as_millis();
    let s = d.as_secs();
    match s {
        0 if ms > 0 => format!("{ms}ms"),
        0..60 => format!("{s}s"),
        60..3600 => format!("{}m{:02}s", s / 60, s % 60),
        _ => format!("{}h{:02}m", s / 3600, (s % 3600) / 60),
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
    if let Some(scale) = heat_scale(area.width, &app.theme) {
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
fn heat_scale(width: u16, theme: &Theme) -> Option<String> {
    // Read from the theme, not from a constant, so the printed numbers cannot
    // drift from the ones the colouring actually uses. That agreement is the
    // whole value of printing them.
    // `{}` rather than `{:.0}`: 50.0 still prints `50`, but 62.5 prints `62.5`
    // instead of rounding to `62` and claiming the colour changes half a point
    // from where it does. Rounding also made `warn = 49.6` print `50`,
    // indistinguishable from the default the user was trying to move off.
    let scale = format!("· warn {} · crit {} ", theme.warn_pct, theme.critical_pct);
    // Room for the state badge. There are no border columns to reserve since
    // L1 — the title is a full-width content line now.
    const RESERVED: usize = 24;
    (width as usize >= scale.chars().count() + RESERVED).then_some(scale)
}

/// Cores drawn between gaps.
const CORE_GROUP: usize = 4;

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
    // Gaps are part of the width now, or the overflow count goes wrong again.
    let with_gaps = |shown: usize| shown + shown.saturating_sub(1) / CORE_GROUP;
    let marker_len = |shown: usize| {
        if shown == n {
            0
        } else {
            format!(" +{}", n - shown).chars().count()
        }
    };
    let mut shown = n.min(avail);
    while with_gaps(shown) + marker_len(shown) > avail {
        if shown == 0 {
            break;
        }
        shown -= 1;
    }
    // Even a bare marker does not fit. State the count alone: drawing meters
    // that cannot say how many are missing is the failure this exists to avoid.
    if with_gaps(shown) + marker_len(shown) > avail {
        return Line::from(Span::styled(format!("{n} cores"), theme.dim_style()));
    }

    let mut spans = vec![Span::styled(label, theme.dim_style())];
    for (i, &pct) in s.cpu_per_core.iter().take(shown).enumerate() {
        // A gap every four. Fourteen cores drawn solid read as one progress
        // bar; grouped, they read as fourteen meters, which is what they are.
        if i > 0 && i % CORE_GROUP == 0 {
            spans.push(Span::raw(" "));
        }
        let idx = ((pct / 100.0 * 7.0).round() as usize).min(7);
        spans.push(Span::styled(BARS[idx].to_string(), theme.heat_style(pct)));
    }
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

    // Gaps are found over the whole buffer, not the window, so a discontinuity
    // falling on the first drawn sample is still seen — within the window it
    // has no predecessor to be discontinuous with.
    let all_times: Vec<std::time::SystemTime> = samples.iter().map(|s| s.at).collect();
    let all_gaps = history::gaps_in(&all_times, app.interval);
    let gap_slots = history::any_slots(&all_gaps[window_start..window_start + shown], zoom, slots);

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
        // Each graph scales to its own peak: memory at 78% and CPU at 16% are
        // different questions and deserve different axes.
        let peak = values.iter().flatten().copied().fold(0.0_f32, f32::max);
        let ceiling = glyphs::ceiling_for(peak);
        // Both thresholds, not just critical. The warn boundary is the one the
        // roadmap actually asked for, and leaving it hue-only kept it invisible
        // to the commonest colour vision deficiency and on any mono terminal.
        let rules: Vec<(usize, usize)> = [app.theme.warn_pct, app.theme.critical_pct]
            .iter()
            .filter_map(|&pct| glyphs::rule_position_scaled(pct, rows, ceiling))
            .collect();
        for row in 0..rows {
            let rule_level = rules.iter().find(|(r, _)| *r == row).map(|(_, l)| *l);
            let mut spans = axis_label(
                row,
                rows,
                gutter,
                &app.theme,
                labelled.then_some(name),
                ceiling,
            );
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
                        ceiling,
                        gaps: &gap_slots,
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
        // Real elapsed time, not sample count. A caption saying `4m32s shown`
        // beside a seam saying `time missing` is the graph contradicting
        // itself in adjacent characters — the window really did span nine
        // minutes, and 272 of those seconds are the gap.
        let span = fmt_lag(
            window
                .first()
                .zip(window.last())
                .and_then(|(a, b)| b.at.duration_since(a.at).ok())
                .unwrap_or_default(),
        );
        let per_slot = fmt_lag(app.interval * zoom as u32);
        // Only the identification half is dropped when the gutter names the
        // series. The span, the slot size and the keys are not a legend and
        // are not duplicated anywhere else.
        let ident = if labelled { "" } else { "cpu · mem — " };
        // Named only when one is on screen. A seam is self-evidently not data,
        // but "time is missing here" is not something a reader can deduce from
        // a dotted line, and a permanent legend entry for something you may
        // never see is clutter charged against every other frame.
        let gap_note = if gap_slots.iter().any(|&g| g) {
            format!(", {} time missing", app.glyphs.gap_glyph())
        } else {
            String::new()
        };
        // Drop the key hints before letting anything be cut mid-word. The
        // scale is a fact about what is on screen and the gap note is a
        // correction to it; the keys are a reminder, and a reminder is the
        // right thing to lose first. Without this the gap note cost about
        // sixteen columns and silently truncated `+/- zoom` on a narrow panel.
        let facts = format!("{ident}{span} shown, {per_slot}/slot{gap_note}");
        let keys = " — ←/→ scrub, +/- zoom";
        let legend = if facts.chars().count() + keys.chars().count() <= inner_w {
            facts + keys
        } else {
            facts
        };
        lines.push(Line::from(Span::styled(legend, app.theme.dim_style())));
    }

    // Retained is what the clock says; capacity is what the buffer will hold at
    // the nominal rate, which is a claim about the future and so is nominal by
    // nature. Mixing a measured figure with a projected one is deliberate.
    let title = format!(
        " timeline — {} of {} buffered ",
        fmt_lag(app.history.span()),
        // `capacity - 1`, for the same reason `history_len` adds one: a buffer
        // of n samples spans n - 1 intervals. `capacity * interval` overstated
        // the span it can hold by exactly one interval.
        fmt_lag(app.interval * app.history.capacity().saturating_sub(1) as u32),
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
    /// Top of the y-axis for this graph.
    ceiling: f32,
    /// Per-slot flags marking where time is missing from the buffer.
    gaps: &'a [bool],
}

/// Draw one row of a graph.
fn glyph_row(g: GraphRow, theme: &Theme) -> Line<'static> {
    let (set, values, row, rows, spc, rule_level, series, ceiling, gaps) = (
        g.set,
        g.values,
        g.row,
        g.rows,
        g.spc,
        g.rule_level,
        g.series,
        g.ceiling,
        g.gaps,
    );
    let spans = values
        .chunks(spc)
        .enumerate()
        .map(|(i, cell)| {
            // A seam where time is missing, drawn full height and in chrome so
            // it cannot be read as a bar.
            //
            // It costs the whole cell — `spc * zoom` samples, so two at the
            // default and sixteen at maximum zoom on a braille terminal. That
            // is a real loss and worth naming: a machine that has just woken
            // or just unwedged is exactly when a spike is likely, and a spike
            // in the sample beside the resumed one is hidden by this.
            //
            // Taken anyway, because the alternative is worse in kind rather
            // than in degree. Compressing twenty minutes of absence into one
            // cell of idle is not a lost sample, it is a graph whose x-axis is
            // untrue, and every reading taken from it after that is wrong. The
            // cost also scales the right way: the more samples a cell covers,
            // the more time the seam is standing for.
            if gaps
                .get(i * spc..(i * spc + spc).min(gaps.len()))
                .is_some_and(|g| g.iter().any(|&f| f))
            {
                return Span::styled(set.gap_glyph().to_string(), theme.chrome_style());
            }
            let pcts: Vec<f32> = cell.iter().map(|v| v.unwrap_or(0.0)).collect();
            let left = glyphs::level_in_row_scaled(pcts[0], row, rows, ceiling);
            let right =
                glyphs::level_in_row_scaled(*pcts.get(1).unwrap_or(&pcts[0]), row, rows, ceiling);
            // Colour is identity here, not magnitude — see `Theme::series_style`.
            // The threshold rules now carry "is this bad", which is what the
            // heat ramp was doing redundantly on top of the bar height.
            // Data always wins the cell. The rule fills gaps only, and dashes
            // so it reads as a reference line rather than a row of samples — at
            // the mono tier a solid rule is indistinguishable from a low bar,
            // since both render a dim `⣀`.
            // A cell with no sample at all is not the same as a cell whose
            // bar does not reach this row. Drawing the rule across the part of
            // the buffer that has not been filled yet is noise about a region
            // where there is nothing to reference.
            let has_data = cell.iter().any(|v| v.is_some());
            let empty = left.max(right) == 0;
            match rule_level {
                Some(lvl) if has_data && empty && i % 2 == 0 => {
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
    ceiling: f32,
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
        // The ceiling, not a fixed 100 — the axis has to say what it is, or
        // scaling it would be the misleading kind of clever.
        format!("{ceiling:.0}")
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
    // Memory bars are scaled against the displayed sample's total, not the
    // live one, so they stay correct while scrubbed like everything else here.
    let total_mem = app.history.current().map_or(0, |s| s.mem.total);

    // Per-process history, for the rows actually on screen.
    //
    // This is the thing no other monitor can draw. Every retained sample holds
    // its whole process list, so "what has *this* process been doing" is
    // already in the buffer — htop, btop and bottom keep no per-process
    // history, zenith's is aggregate-only, and atop has the data but replays
    // whole intervals from a logfile rather than putting a trend beside a row.
    let visible_rows = area.height.saturating_sub(2) as usize;
    let row_offset = app.selected.saturating_sub(visible_rows.saturating_sub(1));
    let keys: Vec<(i32, u64)> = rows_data
        .iter()
        .skip(row_offset)
        .take(visible_rows)
        .map(|r| (r.proc.pid, r.proc.started))
        .collect();
    // The whole retained buffer, not a slice of it. A per-row summary that
    // shifted every time the timeline zoomed would be a second, contradictory
    // reading of the same history; "what this process has been doing" is a
    // fixed question with a fixed answer.
    let series = history::series_for(&app.history, &keys, app.history.len());
    let spark_slots = SPARK_W * app.glyphs.samples_per_cell();
    let spark_zoom = app.history.len().div_ceil(spark_slots.max(1)).max(1);

    // One ceiling across every row. Scaling each sparkline to its own peak
    // makes a flat 12% process look exactly like one spiking to 90%, which
    // defeats the only reason to put them in a column together.
    let spark_ceiling = glyphs::ceiling_for(
        series
            .values()
            .flat_map(|v| v.iter().flatten())
            .copied()
            .fold(0.0_f32, f32::max),
    );
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
                // A bar beside the number turns a column that must be read
                // into one that can be scanned. htop does the same, for the
                // same reason.
                //
                // Both bars are neutral. Length already carries the magnitude,
                // and the number beside each one already carries its status
                // colour — colouring the bar too would spend a third channel
                // on the same fact. Using a series hue here was worse still:
                // that is an identity token, and a share of memory is not an
                // identity. The C6 test caught it.
                Cell::from(cpu_bar(p.cpu)).style(app.theme.dim_style()),
                Cell::from(fmt_bytes(p.rss)),
                Cell::from(glyphs::micro_bar(mem_frac(p.rss, total_mem), BAR_W))
                    .style(app.theme.dim_style()),
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
            // The sparkline sits before the command, so the eye can run down
            // a column of shapes rather than hunting for it past ragged names.
            cells.push(
                Cell::from(sparkline(
                    series.get(&(p.pid, p.started)).map(Vec::as_slice),
                    app.glyphs,
                    spark_zoom,
                    spark_ceiling,
                ))
                .style(app.theme.dim_style()),
            );
            cells.push(Cell::from(Line::from(vec![
                Span::styled(r.prefix.clone(), app.theme.chrome_style()),
                Span::raw(p.name.to_string()),
            ])));
            Row::new(cells).style(style)
        })
        .collect();

    let mut header_cells = vec!["PID", "USER", "CPU%", "", "RSS", "", "S", "THR", "HISTORY"];
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
        Constraint::Length(BAR_W as u16 + 1), // bar, plus room for the over-100 mark
        Constraint::Length(8),
        Constraint::Length(BAR_W as u16),
        Constraint::Length(2),
        Constraint::Length(4),
    ];
    if app.show_io {
        widths.push(Constraint::Length(9));
        widths.push(Constraint::Length(9));
    }
    widths.push(Constraint::Length(SPARK_W as u16));
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

/// Width of the per-process history sparkline, in cells.
const SPARK_W: usize = 10;

/// One process's CPU history as a sparkline.
///
/// Peak aggregation, like the timeline: averaging a spike with idle samples
/// renders it as nothing, and a spike is the entire reason to look.
///
/// A process absent from a sample leaves a gap rather than a zero. "It was not
/// running" and "it was running and idle" are different facts, and a graph that
/// conflates them invents history.
fn sparkline(series: Option<&[Option<f32>]>, set: GlyphSet, zoom: usize, ceiling: f32) -> String {
    let Some(series) = series else {
        return " ".repeat(SPARK_W);
    };
    let spc = set.samples_per_cell();
    let slots = SPARK_W * spc;
    let values: Vec<f32> = series.iter().map(|v| v.unwrap_or(0.0)).collect();
    let agg = history::peak_slots(&values, zoom.max(1), slots);
    agg.chunks(spc)
        .map(|cell| {
            let a = cell[0].unwrap_or(0.0);
            let b = *cell.get(1).and_then(|v| v.as_ref()).unwrap_or(&a);
            let l = glyphs::level_in_row_scaled(a, 0, 1, ceiling);
            let r = glyphs::level_in_row_scaled(b, 0, 1, ceiling);
            set.glyph(l, r)
        })
        .collect()
}

/// Width of a process-table bar. Four cells at eight sub-steps is thirty-two
/// levels, which is enough to compare two rows at a glance.
const BAR_W: usize = 4;

/// The CPU bar, with the over-one-core case marked rather than clipped.
///
/// A threaded process really can use 400% of a core. Clipping it to a full bar
/// would make it indistinguishable from one using exactly 100%, so the excess
/// gets a mark of its own.
fn cpu_bar(pct: f32) -> String {
    let bar = glyphs::micro_bar(pct / 100.0, BAR_W);
    if pct > 100.0 {
        format!("{bar}+")
    } else {
        format!("{bar} ")
    }
}

/// A process's share of the machine's memory.
fn mem_frac(rss: u64, total: u64) -> f32 {
    if total == 0 {
        return 0.0;
    }
    rss as f32 / total as f32
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
