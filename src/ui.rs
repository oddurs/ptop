//! Rendering.
//!
//! Layout, top to bottom: a summary header, per-core meters, the timeline
//! (the reason this tool exists), the process table, and a help line.

use crate::app::App;
use crate::sample::Sample;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use std::time::Duration;

/// Eighth-block glyphs, used to draw the timeline one cell per sample.
const BARS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn draw(f: &mut Frame, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3), // header
        Constraint::Length(3), // cores
        Constraint::Length(6), // timeline
        Constraint::Min(5),    // processes
        Constraint::Length(1), // help
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
    draw_cores(f, chunks[1], sample);
    draw_timeline(f, chunks[2], app);
    draw_procs(f, chunks[3], app);
    draw_help(f, chunks[4], app);
}

fn bordered(title: &str) -> Block<'_> {
    Block::default().borders(Borders::ALL).title(title)
}

/// Green below 50%, yellow to 80%, red above. Same scale everywhere so a colour
/// means the same thing in the meters and in the timeline.
fn heat(pct: f32) -> Color {
    match pct {
        p if p >= 80.0 => Color::Red,
        p if p >= 50.0 => Color::Yellow,
        _ => Color::Green,
    }
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
        Span::styled("CPU ", Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            format!("{:>5.1}%", s.cpu_total),
            Style::default().fg(heat(s.cpu_total)).add_modifier(Modifier::BOLD),
        ),
        Span::raw("   "),
        Span::styled("MEM ", Style::default().add_modifier(Modifier::DIM)),
        Span::styled(
            format!("{:>5.1}%", mem_pct),
            Style::default().fg(heat(mem_pct)).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                " ({} / {}, {} avail)",
                fmt_bytes(s.mem.used),
                fmt_bytes(s.mem.total),
                fmt_bytes(s.mem.available)
            ),
            Style::default().add_modifier(Modifier::DIM),
        ),
    ];

    if s.mem.swap_total > 0 {
        spans.push(Span::styled("   SWP ", Style::default().add_modifier(Modifier::DIM)));
        spans.push(Span::styled(
            format!("{:>5.1}%", s.mem.swap_pct()),
            Style::default().fg(heat(s.mem.swap_pct())),
        ));
    }

    spans.extend([
        Span::styled("   LOAD ", Style::default().add_modifier(Modifier::DIM)),
        Span::raw(format!("{:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2])),
        Span::styled("   UP ", Style::default().add_modifier(Modifier::DIM)),
        Span::raw(fmt_uptime(s.uptime)),
        Span::styled("   PROCS ", Style::default().add_modifier(Modifier::DIM)),
        Span::raw(s.procs.len().to_string()),
    ]);

    let title = if app.history.is_live() {
        Span::styled(" ptop — LIVE ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
    } else {
        // Loud on purpose: reading a stale process table as the current one is
        // the single worst thing this tool could let you do.
        Span::styled(
            format!(" ptop — PAUSED  -{} ", fmt_lag(app.history.time_behind())),
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
        )
    };

    f.render_widget(
        Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

fn draw_cores(f: &mut Frame, area: Rect, s: &Sample) {
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
                Span::styled(BARS[idx].to_string(), Style::default().fg(heat(pct))),
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

/// The scrubable timeline: one column per retained sample, oldest on the left.
fn draw_timeline(f: &mut Frame, area: Rect, app: &App) {
    let inner_w = area.width.saturating_sub(2) as usize;
    if inner_w == 0 {
        return;
    }

    let samples: Vec<&Sample> = app.history.iter().collect();
    // The buffer usually holds more samples than the terminal has columns, so
    // show the most recent window that fits rather than squashing everything.
    let start = samples.len().saturating_sub(inner_w);
    let window = &samples[start..];
    let cursor_col = app.history.cursor_index().saturating_sub(start);

    let mut cpu_row = Vec::with_capacity(window.len());
    let mut mem_row = Vec::with_capacity(window.len());

    for (i, s) in window.iter().enumerate() {
        let on_cursor = i == cursor_col && !app.history.is_live();
        for (row, pct) in [(&mut cpu_row, s.cpu_total), (&mut mem_row, s.mem.used_pct())] {
            let idx = ((pct / 100.0 * 7.0).round() as usize).min(7);
            let mut style = Style::default().fg(heat(pct));
            if on_cursor {
                style = style.bg(Color::White).add_modifier(Modifier::BOLD);
            }
            row.push(Span::styled(BARS[idx].to_string(), style));
        }
    }

    // A caret row under the bars, so the cursor is legible without relying on
    // colour at all.
    let mut marker_row: Vec<Span> = Vec::new();
    if !app.history.is_live() {
        let mut m = vec![' '; window.len()];
        if let Some(slot) = m.get_mut(cursor_col) {
            *slot = '▲';
        }
        marker_row.push(Span::styled(
            m.into_iter().collect::<String>(),
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        ));
    }

    let title = format!(
        " timeline — {}s shown, {}s of {}s buffered ",
        window.len(),
        app.history.len(),
        app.history.capacity()
    );

    let scale_row = if marker_row.is_empty() {
        Line::from(vec![Span::styled(
            format!("{:<width$}now", "past", width = inner_w.saturating_sub(3)),
            Style::default().add_modifier(Modifier::DIM),
        )])
    } else {
        Line::from(marker_row)
    };

    let text = vec![
        Line::from(cpu_row),
        Line::from(mem_row),
        scale_row,
        Line::from(vec![Span::styled(
            "cpu (top) · mem (bottom) — ←/→ scrub, Space live",
            Style::default().add_modifier(Modifier::DIM),
        )]),
    ];

    f.render_widget(Paragraph::new(text).block(bordered(&title)), area);
}

fn draw_procs(f: &mut Frame, area: Rect, app: &App) {
    let procs = app.visible_procs();
    let rows_visible = area.height.saturating_sub(3) as usize;

    // Keep the selected row on screen while scrolling through a long list.
    let offset = app.selected.saturating_sub(rows_visible.saturating_sub(1));

    let rows: Vec<Row> = procs
        .iter()
        .enumerate()
        .skip(offset)
        .take(rows_visible)
        .map(|(i, p)| {
            let style = if i == app.selected {
                Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(p.pid.to_string()),
                Cell::from(p.user.to_string()),
                Cell::from(format!("{:.1}", p.cpu)).style(Style::default().fg(heat(p.cpu))),
                Cell::from(fmt_bytes(p.rss)),
                Cell::from(p.state.to_string()),
                Cell::from(p.threads.to_string()),
                Cell::from(p.name.clone()),
            ])
            .style(style)
        })
        .collect();

    let header = Row::new(vec!["PID", "USER", "CPU%", "RSS", "S", "THR", "COMMAND"])
        .style(Style::default().add_modifier(Modifier::BOLD | Modifier::REVERSED));

    let title = format!(
        " processes ({}) — sort: {} ",
        procs.len(),
        app.sort.label()
    );

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(10),
            Constraint::Length(6),
            Constraint::Length(8),
            Constraint::Length(2),
            Constraint::Length(4),
            Constraint::Min(10),
        ],
    )
    .header(header)
    .block(bordered(&title));

    f.render_widget(table, area);
}

fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let line = if app.editing_filter {
        Line::from(vec![
            Span::styled("filter: ", Style::default().fg(Color::Yellow)),
            Span::raw(&app.filter),
            Span::styled("█", Style::default().fg(Color::Yellow)),
            Span::styled("   (Enter/Esc to finish)", Style::default().add_modifier(Modifier::DIM)),
        ])
    } else {
        Line::from(Span::styled(
            "q quit · ←/→ scrub · Space live · Home oldest · ↑/↓ select · s sort · / filter",
            Style::default().add_modifier(Modifier::DIM),
        ))
    };
    f.render_widget(Paragraph::new(line), area);
}
