//! Measuring a theme against the guarantee the built-in palettes are held to.
//!
//! ptop is the only monitor that measures its own palette: CI fails if any
//! pair of meaning-bearing hues drops below ΔE 8 under Machado 2009 simulation,
//! or if a colour falls below 3:1 against a background it is actually drawn
//! over. That guarantee is the whole point of the colour work.
//!
//! The moment users can supply themes it evaporates — unless the validator is
//! turned outward. So this module is what `--check-theme` prints *and* what the
//! palette tests assert: one implementation, so a contributed theme is measured
//! by the same instrument as the built-ins, and neither can drift from the
//! other by having its own copy.

use crate::cvd::{self, CVD_TARGET, Cvd};
use crate::theme::{Theme, Tier, Token};
use std::fmt;

/// Below this a colour is not reliably legible on the background it sits on.
///
/// WCAG's threshold for large text and graphical objects, which is what a bar
/// glyph and a two-decimal figure are.
pub const MIN_CONTRAST: f64 = 3.0;

/// The panel background ptop draws over.
///
/// A constant rather than a reading of the terminal: ptop never sets a
/// background, so the real one belongs to the user's terminal theme and cannot
/// be known. This is a dark surface typical of the terminals ptop is designed
/// against, and a figure measured against a stated assumption beats no figure.
pub const SURFACE: [u8; 3] = [0x1a, 0x1a, 0x19];

/// The tokens that carry meaning, and so must be told apart.
///
/// Chrome and text are excluded deliberately: they are not asked to be
/// distinguished from each other, only to recede. `live` is excluded because it
/// deliberately shares the `ok` hue.
const MEANINGFUL: [Token; 5] = [
    Token::Ok,
    Token::Warn,
    Token::Critical,
    Token::SeriesCpu,
    Token::SeriesMem,
];

/// How far apart two meaning-bearing colours are, and for whom they are worst.
pub struct Pair {
    pub a: &'static str,
    pub b: &'static str,
    pub delta_e: f64,
    /// The vision under which the pair is closest, or `None` for normal vision.
    ///
    /// Normal vision is included because the tritan matrix has entries above 1,
    /// so a simulation can *increase* separation: a pair too close for everyone
    /// could otherwise pass a deficiency-only check.
    pub worst_for: Option<Cvd>,
}

impl Pair {
    pub fn passes(&self) -> bool {
        self.delta_e >= CVD_TARGET
    }
}

/// Whether a colour can be seen at all on a background it is drawn over.
pub struct Legibility {
    pub token: &'static str,
    pub background: &'static str,
    pub ratio: f64,
}

impl Legibility {
    pub fn passes(&self) -> bool {
        self.ratio >= MIN_CONTRAST
    }
}

/// Everything measurable about one theme.
pub struct Report {
    pub name: String,
    pub tier: Tier,
    pub pairs: Vec<Pair>,
    pub legibility: Vec<Legibility>,
    /// Why a failure is nonetheless intended, when it is.
    ///
    /// `classic` fails on purpose — it exists to restore the green/yellow
    /// convention, and green/yellow is the pair that convention gets wrong.
    /// Reporting that as a bare FAIL would read as ptop failing its own check
    /// rather than as the choice it is.
    pub caveat: Option<&'static str>,
}

impl Report {
    pub fn with_caveat(mut self, caveat: Option<&'static str>) -> Self {
        self.caveat = caveat;
        self
    }

    pub fn of(name: &str, theme: &Theme) -> Self {
        let colours: Vec<(&'static str, [u8; 3])> = MEANINGFUL
            .iter()
            .filter_map(|&t| cvd::to_rgb(t.get(theme)).map(|rgb| (t.name(), rgb)))
            .collect();

        let mut pairs = Vec::new();
        for (i, &(an, a)) in colours.iter().enumerate() {
            for &(bn, b) in &colours[i + 1..] {
                let (delta_e, worst_for) = cvd::worst_cvd(a, b);
                pairs.push(Pair {
                    a: an,
                    b: bn,
                    delta_e,
                    worst_for,
                });
            }
        }

        // Both backgrounds, because a colour is drawn over both and clearing
        // one says nothing about the other. The first 256-colour palette
        // cleared ΔE comfortably while sitting at 2.03:1 on the selected row.
        let selected = cvd::to_rgb(theme.selection_bg).unwrap_or(SURFACE);
        let mut legibility = Vec::new();
        for &(name, rgb) in &colours {
            for (bg_name, bg) in [("surface", SURFACE), ("selected row", selected)] {
                legibility.push(Legibility {
                    token: name,
                    background: bg_name,
                    ratio: cvd::contrast(rgb, bg),
                });
            }
        }

        Self {
            name: name.to_string(),
            tier: theme.tier,
            pairs,
            legibility,
            caveat: None,
        }
    }

    /// The closest pair, which is the number that decides the whole theme.
    pub fn worst_pair(&self) -> Option<&Pair> {
        self.pairs
            .iter()
            .min_by(|x, y| x.delta_e.total_cmp(&y.delta_e))
    }

    pub fn worst_contrast(&self) -> Option<&Legibility> {
        self.legibility
            .iter()
            .min_by(|x, y| x.ratio.total_cmp(&y.ratio))
    }

    pub fn passes(&self) -> bool {
        self.pairs.iter().all(Pair::passes) && self.legibility.iter().all(Legibility::passes)
    }

    /// One line for a theme that loads anyway.
    ///
    /// A failing theme still loads. It is the user's terminal and their choice;
    /// ptop's job is to have the number and say it, not to refuse — the same
    /// principle as rendering `—` rather than a fabricated zero.
    pub fn warning(&self) -> Option<String> {
        if self.passes() {
            return None;
        }
        let mut why = Vec::new();
        if let Some(p) = self.worst_pair().filter(|p| !p.passes()) {
            why.push(format!(
                "{} and {} are only ΔE {:.1} apart{}",
                p.a,
                p.b,
                p.delta_e,
                p.worst_for
                    .map_or(String::new(), |k| format!(" ({k:?})"))
                    .to_lowercase()
            ));
        }
        if let Some(l) = self.worst_contrast().filter(|l| !l.passes()) {
            why.push(format!(
                "{} is {:.2}:1 on the {}",
                l.token, l.ratio, l.background
            ));
        }
        Some(format!(
            "theme `{}`: {} — run `ptop --check-theme {}` for the rest",
            self.name,
            why.join(", "),
            self.name
        ))
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.pairs.is_empty() {
            return writeln!(
                f,
                "{}: nothing to measure — {:?} has no hues to tell apart",
                self.name, self.tier
            );
        }
        writeln!(
            f,
            "{}: {}",
            self.name,
            if self.passes() { "PASS" } else { "FAIL" }
        )?;

        // Every pair and every background, not only the failures. A theme that
        // passes at 8.1 is a different thing from one that passes at 30, and
        // the number is the point — a contributed theme should arrive with a
        // measurement rather than a screenshot.
        for p in &self.pairs {
            let vision = p
                .worst_for
                .map_or("normal".to_string(), |k| format!("{k:?}").to_lowercase());
            let row = format!(
                "  {:<11} ↔ {:<11} ΔE {:5.1}  {vision:<8}",
                p.a, p.b, p.delta_e
            );
            match p.passes() {
                true => writeln!(f, "{}", row.trim_end())?,
                false => writeln!(f, "{row}  below the target of {CVD_TARGET:.0}")?,
            }
        }
        for l in &self.legibility {
            let row = format!(
                "  {:<11} on {:<13} {:>6.2}:1        ",
                l.token, l.background, l.ratio
            );
            match l.passes() {
                true => writeln!(f, "{}", row.trim_end())?,
                false => writeln!(f, "{row}  below {MIN_CONTRAST:.0}:1")?,
            }
        }
        let worst_pair = self.worst_pair().map_or(0.0, |p| p.delta_e);
        let worst_contrast = self.worst_contrast().map_or(0.0, |l| l.ratio);
        writeln!(
            f,
            "  worst pair: ΔE {worst_pair:.1}   worst contrast: {worst_contrast:.2}:1"
        )?;
        let Some(why) = self.caveat else {
            return Ok(());
        };
        writeln!(f)?;
        // Wrapped here rather than left as one long line: the rest of the
        // report is a column layout, and a paragraph running off the right of
        // it undoes the reason the columns are there.
        for line in wrap(why, 74) {
            writeln!(f, "  {line}")?;
        }
        Ok(())
    }
}

/// Greedy wrap to `width` columns.
///
/// Hand-rolled because the alternative is a dependency for eight lines, in a
/// project whose `/proc` parser is hand-rolled for the same reason.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = vec![String::new()];
    for word in text.split_whitespace() {
        let line = lines.last_mut().expect("never empty");
        if line.is_empty() {
            *line = word.to_string();
        } else if line.chars().count() + 1 + word.chars().count() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            lines.push(word.to_string());
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Palette, Token};
    use ratatui::style::Color;

    fn themed(overrides: &[(Token, Color)]) -> Theme {
        Theme::new(Palette::Safe, Tier::TrueColor)
            .with_overrides(overrides)
            .0
    }

    #[test]
    fn every_pair_and_both_backgrounds_are_reported() {
        // Five meaning-bearing colours is ten pairs, and each is measured
        // against both surfaces it is drawn over. A report that showed only
        // failures could not tell a theme passing at 8.1 from one at 30, and
        // the number is the point.
        let report = Report::of("safe", &themed(&[]));
        assert_eq!(report.pairs.len(), 10);
        assert_eq!(report.legibility.len(), 10);
        // Counted in the rendered text, not just in the data. Asserting that
        // token names appear is satisfied by the contrast rows alone, so a
        // report that printed only its failures still passed.
        let text = report.to_string();
        assert_eq!(
            text.lines().filter(|l| l.contains('↔')).count(),
            10,
            "not every pair was printed:\n{text}"
        );
        assert_eq!(
            text.lines().filter(|l| l.contains(" on ")).count(),
            10,
            "not every background was printed:\n{text}"
        );
        for token in ["ok", "warn", "critical", "series_cpu", "series_mem"] {
            assert!(text.contains(token), "{token} missing from:\n{text}");
        }
        assert!(
            text.contains("surface") && text.contains("selected row"),
            "{text}"
        );
        assert!(
            text.contains("worst pair") && text.contains("worst contrast"),
            "{text}"
        );
    }

    #[test]
    fn a_theme_that_cannot_be_told_apart_fails_and_names_the_pair() {
        // Two identical hues is the extreme of the thing being measured.
        let report = Report::of(
            "flat",
            &themed(&[
                (Token::Ok, Color::Rgb(0x5c, 0xcf, 0xe6)),
                (Token::Warn, Color::Rgb(0x5c, 0xcf, 0xe6)),
            ]),
        );
        assert!(!report.passes());
        let worst = report.worst_pair().unwrap();
        assert_eq!((worst.a, worst.b), ("ok", "warn"));
        assert!(
            worst.delta_e < 0.001,
            "identical hues measured {} apart",
            worst.delta_e
        );
        assert!(report.to_string().contains("FAIL"));
    }

    #[test]
    fn an_illegible_colour_fails_on_the_background_it_is_drawn_over() {
        // Separation between hues says nothing about being visible at all: the
        // first 256-colour palette cleared ΔE while sitting at 2.03:1 on the
        // selected row.
        let report = Report::of(
            "dark",
            &themed(&[(Token::Critical, Color::Rgb(0x22, 0x22, 0x22))]),
        );
        assert!(!report.passes());
        let worst = report.worst_contrast().unwrap();
        assert_eq!(worst.token, "critical");
        assert!(worst.ratio < MIN_CONTRAST);
    }

    #[test]
    fn a_failing_theme_still_loads_and_says_why_once() {
        // It is the user's terminal and their choice; ptop's job is to have the
        // number and say it, not to refuse — the same principle as rendering
        // `—` rather than a fabricated zero.
        let report = Report::of(
            "flat",
            &themed(&[(Token::Warn, Color::Rgb(0x5c, 0xcf, 0xe6))]),
        );
        let warning = report.warning().expect("a failing theme should say so");
        assert!(warning.contains("flat"), "{warning}");
        assert!(warning.contains("--check-theme flat"), "{warning}");
        assert_eq!(warning.lines().count(), 1, "more than one line: {warning}");

        // …and a passing theme says nothing at all.
        assert_eq!(Report::of("safe", &themed(&[])).warning(), None);
    }

    #[test]
    fn normal_vision_can_be_the_worst_case() {
        // The tritan matrix has entries above 1, so a simulation can *increase*
        // separation. These two blues are ΔE 5.1 apart to normal vision and
        // further apart to every deficiency — so a deficiency-only check would
        // pass a pair nobody can tell apart, and a report that could not name
        // normal vision would print a number without its subject.
        let report = Report::of(
            "blues",
            &themed(&[
                (Token::Ok, Color::Rgb(0x00, 0x66, 0xbb)),
                (Token::Warn, Color::Rgb(0x00, 0x77, 0xcc)),
            ]),
        );
        let pair = report
            .pairs
            .iter()
            .find(|p| (p.a, p.b) == ("ok", "warn"))
            .unwrap();
        assert_eq!(pair.worst_for, None, "normal vision was not the worst case");
        assert!(!pair.passes(), "ΔE {:.1} should fail", pair.delta_e);
        assert!(report.to_string().contains("normal"), "{report}");
    }

    #[test]
    fn a_monochrome_theme_has_nothing_to_measure() {
        let report = Report::of("safe", &Theme::new(Palette::Safe, Tier::Mono));
        assert!(report.pairs.is_empty());
        assert!(report.passes(), "a report with no measurements cannot fail");
        assert!(report.to_string().contains("nothing to measure"));
    }
}
