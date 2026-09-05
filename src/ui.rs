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
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use std::time::Duration;

/// Eighth-block glyphs, used to draw the timeline one cell per sample.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Length(3),  // cores
        Constraint::Length(10), // timeline
        Constraint::Min(5),     // processes
        Constraint::Length(1),  // help
    ])
    .split(f.area());

    let Some(sample) = app.history.current() else {
        f.render_widget(
            Paragraph::new("collecting first sample…").block(bordered("ptop")),
            f.area(),
        );
        return;
    };

    draw_header(f, chunks[0], app, sample);
    draw_cores(f, chunks[1], sample, &app.theme);
    draw_timeline(f, chunks[2], app);
    draw_procs(f, chunks[3], app);
    draw_help(f, chunks[4], app);
}

fn bordered(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
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

    let title = if app.history.is_live() {
        Span::styled(" ptop — LIVE ", app.theme.live_style())
    } else {
        // Loud on purpose: reading a stale process table as the current one is
        // the single worst thing this tool could let you do.
        Span::styled(
            format!(" ptop — PAUSED  -{} ", fmt_lag(app.history.time_behind())),
            app.theme.paused_style(),
        )
    };

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_cores(f: &mut Frame, area: Rect, s: &Sample, theme: &Theme) {
    if s.cpu_per_core.is_empty() {
        return;
    }
    // One glyph per core, so a 128-core box still fits on one line.
    let spans: Vec<Span> = s
        .cpu_per_core
        .iter()
        .flat_map(|&pct| {
            let idx = ((pct / 100.0 * 7.0).round() as usize).min(7);
            [
                Span::styled(BARS[idx].to_string(), theme.heat_style(pct)),
                Span::raw(" "),
            ]
        })
        .collect();

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(bordered(&format!(" cores ({}) ", s.cpu_per_core.len()))),
        area,
    );
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
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;
    if inner_w == 0 || inner_h == 0 {
        return;
    }

    // Reserve the cursor marker and legend, then divide the rest exactly so no
    // row is left blank. CPU takes the larger share as the spikier signal.
    let graph_rows = inner_h.saturating_sub(2).max(1);
    let cpu_rows = (graph_rows * 3 / 5).max(1);
    let mem_rows = graph_rows.saturating_sub(cpu_rows);

    let spc = app.glyphs.samples_per_cell();
    let slots = inner_w * spc;
    let samples: Vec<&Sample> = app.history.iter().collect();
    let zoom = app::effective_zoom(app.zoom(), samples.len(), slots);
    let shown = (slots * zoom).min(samples.len());
    let window = &samples[samples.len() - shown..];

    let cpu: Vec<f32> = window.iter().map(|s| s.cpu_total).collect();
    let mem: Vec<f32> = window.iter().map(|s| s.mem.used_pct()).collect();
    let cpu_slots = history::peak_slots(&cpu, zoom, slots);
    let mem_slots = history::peak_slots(&mem, zoom, slots);

    let mut lines: Vec<Line> = Vec::with_capacity(inner_h);
    for (values, rows) in [(&cpu_slots, cpu_rows), (&mem_slots, mem_rows)] {
        for row in 0..rows {
            lines.push(glyph_row(app.glyphs, values, row, rows, spc, &app.theme));
        }
    }

    lines.push(cursor_row(app, window.len(), zoom, slots, spc, inner_w));

    if lines.len() < inner_h {
        let span = fmt_lag(Duration::from_secs(window.len() as u64));
        lines.push(Line::from(Span::styled(
            format!("cpu · mem — {span} shown, {zoom}s/slot — ←/→ scrub, +/- zoom"),
            app.theme.dim_style(),
        )));
    }

    let title = format!(
        " timeline — {} of {} buffered ",
        fmt_lag(Duration::from_secs(app.history.len() as u64)),
        fmt_lag(Duration::from_secs(app.history.capacity() as u64)),
    );

    f.render_widget(Paragraph::new(lines).block(bordered(&title)), area);
}

/// One row of graph. `row` counts from the top of a `rows`-tall graph.
fn glyph_row(
    set: GlyphSet,
    values: &[Option<f32>],
    row: usize,
    rows: usize,
    spc: usize,
    theme: &Theme,
) -> Line<'static> {
    let spans = values
        .chunks(spc)
        .map(|cell| {
            let pcts: Vec<f32> = cell.iter().map(|v| v.unwrap_or(0.0)).collect();
            let left = glyphs::level_in_row(pcts[0], row, rows);
            let right = glyphs::level_in_row(*pcts.get(1).unwrap_or(&pcts[0]), row, rows);
            // Colour by the peak of the pair: a cell holding a spike and an
            // idle sample should read as hot, not as lukewarm.
            let peak = pcts.iter().copied().fold(0.0_f32, f32::max);
            Span::styled(set.glyph(left, right).to_string(), theme.heat_style(peak))
        })
        .collect::<Vec<_>>();
    Line::from(spans)
}

/// The row under the graph marking where the scrub cursor sits.
///
/// When a cell holds two samples the marker picks the correct half, so packing
/// never costs cursor precision.
fn cursor_row(
    app: &App,
    n_values: usize,
    zoom: usize,
    slots: usize,
    spc: usize,
    inner_w: usize,
) -> Line<'static> {
    if app.history.is_live() || n_values == 0 {
        return Line::from(Span::styled(
            format!("{:<width$}now", "past", width = inner_w.saturating_sub(3)),
            app.theme.dim_style(),
        ));
    }

    // The cursor is an index into the whole buffer; the graph shows a window of
    // the newest `n_values`, so rebase before locating it.
    let dropped = app.history.len().saturating_sub(n_values);
    let idx = app.history.cursor_index().saturating_sub(dropped);
    let slot = history::slot_of_index(idx, n_values, zoom, slots);

    let mut row = vec![' '; inner_w];
    if let Some(cell) = row.get_mut(slot / spc) {
        *cell = app.glyphs.cursor_marker(spc == 2 && slot % spc == 1);
    }
    Line::from(Span::styled(
        row.into_iter().collect::<String>(),
        app.theme.cursor_style(),
    ))
}

fn draw_procs(f: &mut Frame, area: Rect, app: &App) {
    let rows_data = app.visible_rows();
    let collected = app.history.current().is_some_and(|s| s.io_collected);
    let rows_visible = area.height.saturating_sub(3) as usize;

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
            cells.push(Cell::from(format!("{}{}", r.prefix, p.name)));
            Row::new(cells).style(style)
        })
        .collect();

    let mut header_cells = vec!["PID", "USER", "CPU%", "RSS", "S", "THR"];
    if app.show_io {
        header_cells.extend(["DISK R", "DISK W"]);
    }
    header_cells.push("COMMAND");
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));

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

    let table = Table::new(rows, widths)
        .header(header)
        .block(bordered(&title));

    f.render_widget(table, area);
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
