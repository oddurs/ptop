//! Semantic colour tokens, resolved per colour tier.
//!
//! Nothing outside this module names a colour. Call sites ask for the *job* a
//! colour does — "this is a critical value", "this row is selected" — so the
//! palette can change in one place, and so a reviewer can see at a glance
//! whether a colour is carrying meaning or decoration.
//!
//! Two ideas hold this together.
//!
//! **Status versus identity.** Status tokens (`ok`, `warn`, `critical`) mean a
//! state and are reserved for it. Identity tokens (`series_cpu`, `series_mem`)
//! mean *which thing this is* and imply no judgement. Reusing a status colour
//! as a series colour destroys the meaning of the status colour everywhere else.
//!
//! **Tiers, each independently complete.** Monochrome must be usable, ANSI-16
//! readable, 256/true-colour intended. Monochrome is not a courtesy: it is the
//! proof that no meaning is carried by colour alone. If the display only works
//! in colour, no palette choice can fix that — so every token that carries
//! meaning also has a modifier fallback, and the tier decides which is used.

use ratatui::style::{Color, Modifier, Style};

// # Which token belongs where
//
// Every colour in ptop does exactly one of four jobs, and the jobs do not share
// hues. The rule that costs most to break is the first: a status colour reused
// as a series colour destroys the meaning of that status colour *everywhere
// else in the UI*, because the reader can no longer tell whether red means
// "this is bad" or merely "this is the third thing".
//
// | Job      | Tokens                     | Means                      | Drawn by |
// |----------|----------------------------|----------------------------|----------|
// | Status   | `ok` `warn` `critical`     | a state — is this bad now? | header figures, core meters, the process table's CPU column |
// | Identity | `series_cpu` `series_mem`  | which thing this is        | the timeline graphs |
// | Chrome   | `chrome` `text` `text_dim` | structure, not data        | borders, titles, axis labels, the tree spine, the help line |
// | Cursor   | (no hue — reverse video)   | where you are              | the PAUSED badge, the scrub marker, the filter prompt |
//
// Two fields sit outside the table deliberately. `live` **is** the `ok` hue:
// live-versus-paused is a state, so the LIVE badge is a status use of a status
// colour rather than a fourth job. `selection_bg` is a background rather than a
// hue carrying meaning, and pairs with `text` so a selected row stays legible
// on a light terminal.
//
// The split between status and identity is what G5 acted on: bar height already
// encodes magnitude, so colouring a graph by its own value spends the only free
// channel on information the chart is already showing.
//
// Enforced by `no_palette_reuses_a_status_hue_for_identity` and
// `identity_hues_clear_both_status_palettes` here, and by
// `status_and_identity_hues_stay_in_their_own_panels` in `ui_tests`.

/// How much colour the terminal can be trusted with.
///
/// Ordered, so "can this terminal show that colour" is a comparison rather
/// than a match. The variants are declared least to most capable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Tier {
    /// No colour at all. Meaning is carried by weight, reverse video and glyphs.
    Mono,
    /// The 16 ANSI slots. Note these are *slots*, not colours: the user's
    /// terminal theme decides what `Green` looks like, so this tier can promise
    /// nothing about contrast or colour-vision separation.
    Ansi16,
    /// 256-colour cube. The first tier where ptop actually controls the hues.
    Ansi256,
    #[default]
    /// 24-bit.
    ///
    /// Not simply "256 at full precision": the indexed palette is searched
    /// within the cube under its own contrast and separation constraints, so
    /// its slots sit some distance from their true-colour counterparts. Same
    /// design intent, independently solved for each tier.
    TrueColor,
}

impl Tier {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mono" => Some(Self::Mono),
            "16" => Some(Self::Ansi16),
            "256" => Some(Self::Ansi256),
            "true" => Some(Self::TrueColor),
            _ => None,
        }
    }

    /// Best tier for the current environment.
    ///
    /// Honours `NO_COLOR` (<https://no-color.org>), which is a convention worth
    /// respecting even though almost nothing in this space does.
    pub fn detect() -> Self {
        let env = |k: &str| std::env::var(k).ok();
        Self::detect_from(
            // Non-empty, per the spec: `export NO_COLOR=""` is how a profile
            // opts *back in* to colour, and must not force monochrome.
            env("NO_COLOR").is_some_and(|v| !v.is_empty()),
            env("COLORTERM").as_deref(),
            env("TERM").as_deref(),
        )
    }

    /// Split out so the rules are testable without touching process env.
    pub fn detect_from(no_color: bool, colorterm: Option<&str>, term: Option<&str>) -> Self {
        if no_color || matches!(term, Some("dumb") | None) {
            return Self::Mono;
        }
        match colorterm {
            Some("truecolor" | "24bit") => Self::TrueColor,
            _ if term.is_some_and(|t| t.contains("256color")) => Self::Ansi256,
            _ => Self::Ansi16,
        }
    }

    fn has_color(self) -> bool {
        self != Self::Mono
    }
}

/// Which set of hues to use. Orthogonal to [`Tier`], which is how many colours
/// the terminal can render.
///
/// # Both palettes share their identity hues
///
/// "Green means good" is a convention about **status**. It says nothing about
/// what colour a CPU line should be, so there is no classic answer for the
/// series tokens, and inventing one would mean validating a second pair of hues
/// for no benefit.
///
/// Measured against the classic status hues under Machado 2009, the shared
/// indigo/mauve pair separates by at least ΔE 10.0 at true colour and 12.0 on
/// the 256 cube, against a target of 8. Enforced by
/// `identity_hues_clear_both_status_palettes`.
///
/// This matters because G5 moved the timeline onto these tokens. Before that
/// they were placeholders equal to `ok`, so shipping G5 without separating them
/// drew CPU and MEM in one hue — the hue that also means "good", so a machine
/// at 95% drew its timeline in green.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Palette {
    /// Cyan / amber / red. The default.
    ///
    /// Green-and-red is the worst available pair for red-green colour vision
    /// deficiency, which affects roughly 8% of men — and every system monitor
    /// ships it. Measured against a dark surface in OKLab ΔE×100 under Machado
    /// 2009 simulation, ptop's old green↔yellow separated by **3.7** under
    /// protanopia. Replacing green with cyan takes the worst pair to 16.2.
    #[default]
    Safe,
    /// The green / yellow / red every other monitor uses.
    ///
    /// Kept because green-means-good is a strong convention and breaking it has
    /// a real cost for the majority who can see it. This is the escape hatch,
    /// not the default — and the monochrome tier matters more than either
    /// choice, since a display legible only in colour cannot be fixed by
    /// picking better hues.
    Classic,
}

impl Palette {
    /// The name this palette is written as, so a resolved theme can say which
    /// built-in it is without the caller matching on the enum.
    pub fn name(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Classic => "classic",
        }
    }

    /// Why this palette knowingly fails the separation target, if it does.
    ///
    /// Stated by the palette rather than discovered by the checker, so
    /// `--check-theme classic` reads as the choice it is rather than as ptop
    /// failing its own check.
    pub fn caveat(self) -> Option<&'static str> {
        match self {
            Self::Safe => None,
            Self::Classic => Some(
                "`classic` is knowingly below the target: it restores the green/yellow \
                 convention, and green/yellow is the pair that convention gets wrong under \
                 red-green deficiency, which affects roughly 8% of men. `safe` is the \
                 default for exactly this reason.",
            ),
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "safe" => Some(Self::Safe),
            "classic" => Some(Self::Classic),
            _ => None,
        }
    }
}

/// Named colours, grouped by the job each one does.
// `Eq` would need the thresholds to be integers. They are compared against
// percentages, which are not, and a float threshold is the honest type for a
// value the user writes as `warn = 62.5`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub tier: Tier,

    // Status — a state, never an identity.
    pub ok: Color,
    pub warn: Color,
    pub critical: Color,

    // Identity — which series this is, never a judgement about it.
    pub series_cpu: Color,
    pub series_mem: Color,

    // Chrome and text. Deliberately recessive.
    pub chrome: Color,
    pub text: Color,
    pub text_dim: Color,

    // Interaction.
    //
    // There is deliberately no cursor colour. Attention affordances use reverse
    // video at every tier: it needs no hue (the five meaning-bearing ones are
    // already spoken for), and it is the only emphasis that survives both a
    // light and a dark terminal — which an ANSI slot cannot promise, since the
    // user's theme chooses it.
    pub selection_bg: Color,
    pub live: Color,

    /// Where "getting busy" and "in trouble" begin, as percentages.
    ///
    /// On the theme rather than beside it because they are read wherever a
    /// status hue is chosen — `heat`, `figure_style`, the timeline's threshold
    /// rules and the header legend that prints them — and a threshold that
    /// disagreed with the colour it selects would be worse than no threshold.
    /// A build box and a latency-sensitive service care at very different
    /// levels, and 50/80 was only ever a guess about which one you are.
    pub warn_pct: f32,
    pub critical_pct: f32,
}

impl Theme {
    pub const DEFAULT_WARN_PCT: f32 = 50.0;
    pub const DEFAULT_CRITICAL_PCT: f32 = 80.0;

    /// The same theme with the user's thresholds.
    ///
    /// Applied after construction rather than threaded through seven `const
    /// fn` palettes, which are about hue and have nothing to say about where a
    /// number becomes worrying.
    pub fn with_thresholds(mut self, warn: f32, critical: f32) -> Self {
        self.warn_pct = warn;
        self.critical_pct = critical;
        self
    }

    /// The palette for a tier.
    /// Monochrome ignores the palette entirely — there are no hues to choose
    /// between — which is the clearest statement that meaning never rests on
    /// colour here.
    pub fn new(palette: Palette, tier: Tier) -> Self {
        match (palette, tier) {
            (_, Tier::Mono) => Self::mono(),
            (Palette::Safe, Tier::Ansi16) => Self::safe_ansi16(),
            (Palette::Safe, Tier::Ansi256) => Self::safe_indexed(),
            (Palette::Safe, Tier::TrueColor) => Self::safe_truecolor(),
            (Palette::Classic, Tier::Ansi16) => Self::ansi16(),
            (Palette::Classic, Tier::Ansi256) => Self::indexed(),
            (Palette::Classic, Tier::TrueColor) => Self::truecolor(),
        }
    }

    /// Colour-vision-safe on ANSI slots. `LightBlue` rather than `Blue`: plain
    /// ANSI blue is the lowest-contrast slot against black on most themes.
    const fn safe_ansi16() -> Self {
        Self {
            tier: Tier::Ansi16,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Cyan,
            warn: Color::Yellow,
            critical: Color::Red,
            series_cpu: Color::LightBlue,
            series_mem: Color::Magenta,
            chrome: Color::DarkGray,
            text: Color::Reset,
            // Gray, not DarkGray: chrome takes DarkGray, and collapsing both
            // onto one slot destroys the border/title hierarchy at this tier.
            text_dim: Color::Gray,
            selection_bg: Color::DarkGray,
            live: Color::Cyan,
        }
    }

    /// Colour-vision-safe on the xterm 256 palette.
    ///
    /// Not a per-channel quantisation of the true-colour values. Rounding each
    /// channel to the nearest cube level independently is not perceptually
    /// safe: it collapsed `series_cpu` and `series_mem` from ΔE 12.8 to **6.7**
    /// — below the target — because both landed on neighbouring cube cells.
    /// These indices were instead searched for directly, maximising the worst
    /// pair subject to staying near the intended hue and clearing 3:1 contrast
    /// against **both** the surface and the selected-row background — a first
    /// attempt satisfied only the surface and put `series_cpu` at 2.03:1 on a
    /// selected row, which is invisible. Both figures are enforced below rather
    /// than quoted here, because a number in a comment is not a guarantee.
    const fn safe_indexed() -> Self {
        Self {
            tier: Tier::Ansi256,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Indexed(80),
            warn: Color::Indexed(222),
            critical: Color::Indexed(203),
            series_cpu: Color::Indexed(67),
            series_mem: Color::Indexed(135),
            chrome: Color::Indexed(239),
            text: Color::Indexed(252),
            text_dim: Color::Indexed(244),
            selection_bg: Color::Indexed(237),
            live: Color::Indexed(80),
        }
    }

    /// Colour-vision-safe, at full precision. These are the measured values.
    ///
    /// All ten pairs among the five meaning-bearing hues separate by at least
    /// **ΔE 10.3** (worst: `critical` ↔ `series_mem`) under Machado 2009
    /// simulation at severity 1.0, in OKLab ×100, against a target of 8.
    ///
    /// The model is named on purpose. An independent check using Viénot 1999
    /// put an earlier candidate at 7.95 where Machado gave 9.1, so a figure
    /// quoted without its model is not reproducible. `series_cpu` was moved to
    /// indigo specifically to widen its gap from the cyan `ok` token — 9.1 to
    /// 20.0 — so the palette clears the threshold under either model instead of
    /// sitting on it.
    ///
    /// C4 turns this comment into an enforced test. Until then it is a claim,
    /// not an invariant.
    const fn safe_truecolor() -> Self {
        Self {
            tier: Tier::TrueColor,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Rgb(0x5c, 0xcf, 0xe6),
            warn: Color::Rgb(0xff, 0xd5, 0x80),
            critical: Color::Rgb(0xff, 0x66, 0x66),
            series_cpu: Color::Rgb(0x7a, 0x7a, 0xe6),
            series_mem: Color::Rgb(0xb4, 0x8e, 0xad),
            chrome: Color::Rgb(0x50, 0x50, 0x50),
            text: Color::Rgb(0xcc, 0xcc, 0xcc),
            text_dim: Color::Rgb(0x80, 0x80, 0x80),
            selection_bg: Color::Rgb(0x3a, 0x3a, 0x3a),
            live: Color::Rgb(0x5c, 0xcf, 0xe6),
        }
    }

    /// Every colour is `Reset`. Meaning survives through the modifier
    /// fallbacks on the style helpers below, never through these fields.
    const fn mono() -> Self {
        Self {
            tier: Tier::Mono,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Reset,
            warn: Color::Reset,
            critical: Color::Reset,
            series_cpu: Color::Reset,
            series_mem: Color::Reset,
            chrome: Color::Reset,
            text: Color::Reset,
            text_dim: Color::Reset,
            selection_bg: Color::Reset,
            live: Color::Reset,
        }
    }

    /// The palette ptop shipped before tiers existed.
    const fn ansi16() -> Self {
        Self {
            tier: Tier::Ansi16,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Green,
            warn: Color::Yellow,
            critical: Color::Red,
            // Shared with the safe palette; see [`Palette`].
            series_cpu: Color::LightBlue,
            series_mem: Color::Magenta,
            chrome: Color::DarkGray,
            text: Color::Reset,
            // Gray, not DarkGray: chrome takes DarkGray, and collapsing both
            // onto one slot destroys the border/title hierarchy at this tier.
            text_dim: Color::Gray,
            selection_bg: Color::DarkGray,
            live: Color::Green,
        }
    }

    /// The same hues quantised onto the xterm 256 palette.
    ///
    /// This tier exists precisely because `TERM=*256color` with no `COLORTERM`
    /// — tmux, PuTTY, older xterm — cannot parse a 24-bit escape. Emitting
    /// `Color::Rgb` here would send `ESC[38;2;…` to the one set of terminals
    /// that motivated having the tier at all.
    const fn indexed() -> Self {
        Self {
            tier: Tier::Ansi256,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Indexed(114),
            warn: Color::Indexed(179),
            critical: Color::Indexed(167),
            // Shared with the safe palette; see [`Palette`].
            series_cpu: Color::Indexed(67),
            series_mem: Color::Indexed(135),
            chrome: Color::Indexed(239),
            text: Color::Indexed(252),
            text_dim: Color::Indexed(244),
            selection_bg: Color::Indexed(237),
            live: Color::Indexed(114),
        }
    }

    /// Explicit RGB, so the hues are ptop's rather than the terminal theme's.
    ///
    /// These are still the classic green/yellow/red. Fixing the colour-vision
    /// problem with them is C3's job, and it depends on this tier existing to
    /// have anywhere to put the replacement.
    const fn truecolor() -> Self {
        Self {
            tier: Tier::TrueColor,
            warn_pct: Self::DEFAULT_WARN_PCT,
            critical_pct: Self::DEFAULT_CRITICAL_PCT,
            ok: Color::Rgb(0x77, 0xca, 0x9b),
            warn: Color::Rgb(0xcb, 0xc0, 0x6c),
            critical: Color::Rgb(0xdc, 0x4c, 0x4c),
            // Shared with the safe palette; see [`Palette`].
            series_cpu: Color::Rgb(0x7a, 0x7a, 0xe6),
            series_mem: Color::Rgb(0xb4, 0x8e, 0xad),
            chrome: Color::Rgb(0x50, 0x50, 0x50),
            text: Color::Rgb(0xcc, 0xcc, 0xcc),
            text_dim: Color::Rgb(0x80, 0x80, 0x80),
            selection_bg: Color::Rgb(0x3a, 0x3a, 0x3a),
            live: Color::Rgb(0x77, 0xca, 0x9b),
        }
    }

    /// Status colour for a percentage, on the theme's configured thresholds.
    pub fn heat(&self, pct: f32) -> Color {
        match pct {
            p if p >= self.critical_pct => self.critical,
            p if p >= self.warn_pct => self.warn,
            _ => self.ok,
        }
    }

    /// Style for a value at `pct`.
    ///
    /// Without colour, severity is carried by weight: critical is bold, warn is
    /// normal, ok is dim. That ordering is deliberate — it survives greyscale,
    /// which the green/yellow/red hues do not, since yellow is lighter than
    /// green and red is darker than both.
    pub fn heat_style(&self, pct: f32) -> Style {
        if self.tier.has_color() {
            return Style::default().fg(self.heat(pct));
        }
        match pct {
            p if p >= self.critical_pct => Style::default().add_modifier(Modifier::BOLD),
            p if p >= self.warn_pct => Style::default(),
            _ => Style::default().add_modifier(Modifier::DIM),
        }
    }

    /// A headline figure in the header.
    ///
    /// Not `heat_style().bold()`: at the mono tier severity *is* weight, so
    /// adding bold on top collapses warn and critical into the same style and
    /// the header stops distinguishing a busy machine from a dying one. With
    /// colour the hue carries severity and bold is free to mean "this is a
    /// headline figure".
    pub fn figure_style(&self, pct: f32) -> Style {
        if self.tier.has_color() {
            self.heat_style(pct).add_modifier(Modifier::BOLD)
        } else {
            self.heat_style(pct)
        }
    }

    /// The PAUSED badge. Loud on purpose in every tier: reading a stale process
    /// table as the current one is the worst thing this tool could allow.
    /// Reverse video rather than a colour pair: a white-on-something badge
    /// disappears on a light terminal, and no ANSI slot is safe on both.
    pub fn paused_style(&self) -> Style {
        Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED)
    }

    pub fn live_style(&self) -> Style {
        let base = Style::default().add_modifier(Modifier::BOLD);
        if self.tier.has_color() {
            base.fg(self.live)
        } else {
            base
        }
    }

    /// The selected table row. Reverse video without colour, which is how
    /// selection has always been shown on a monochrome terminal.
    pub fn selection_style(&self) -> Style {
        let base = Style::default().add_modifier(Modifier::BOLD);
        if self.tier.has_color() {
            // Foreground as well as background. A background alone inherits
            // the terminal's default foreground, which on a light theme is
            // dark — rendering the selected row dark-on-dark and invisible.
            base.bg(self.selection_bg).fg(self.text)
        } else {
            base.add_modifier(Modifier::REVERSED)
        }
    }

    /// The scrub cursor. Its glyph already distinguishes it; this is emphasis.
    pub fn cursor_style(&self) -> Style {
        Style::default()
            .add_modifier(Modifier::BOLD)
            .add_modifier(Modifier::REVERSED)
    }

    /// One series of a graph, by identity rather than by value.
    ///
    /// Bar height already encodes magnitude; colouring by the same number is
    /// double-encoding, and it spends the one free channel on information the
    /// chart is already showing. Identity is the job height cannot do.
    ///
    /// Without colour the series are told apart by position and by the gutter
    /// labels, which is why those had to land first.
    pub fn series_style(&self, series: Color) -> Style {
        if self.tier.has_color() {
            Style::default().fg(series)
        } else {
            Style::default()
        }
    }

    /// Panel borders and rules.
    ///
    /// The most recessive thing on screen. Chrome should be findable when
    /// looked for and invisible when not — it competes with the data for
    /// attention otherwise, and the data is the point.
    pub fn chrome_style(&self) -> Style {
        if self.tier.has_color() {
            Style::default().fg(self.chrome)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        }
    }

    /// The process table's header row.
    ///
    /// Bold and underlined rather than reverse video. A full-width reversed bar
    /// is the loudest thing on screen, and it is a column label — it should not
    /// outweigh the processes beneath it. Reverse video stays at the mono tier,
    /// where weight alone is not enough separation.
    pub fn table_header_style(&self) -> Style {
        let base = Style::default().add_modifier(Modifier::BOLD);
        if self.tier.has_color() {
            base.add_modifier(Modifier::UNDERLINED).fg(self.text_dim)
        } else {
            base.add_modifier(Modifier::REVERSED)
        }
    }

    /// Panel titles. Readable — they carry real information — but a step back
    /// from the figures inside the panel.
    pub fn title_style(&self) -> Style {
        self.dim_style()
    }

    /// Recessive text: labels, legends, the help line.
    ///
    /// One mechanism, not both. Terminals implement `DIM` by blending toward
    /// the background, so a dim *hue* plus the dim *modifier* compounds into
    /// roughly #505050 on black — a contrast regression, in the change whose
    /// whole purpose is accessibility. With colour the hue does the work;
    /// without it, the modifier does.
    pub fn dim_style(&self) -> Style {
        if self.tier.has_color() {
            Style::default().fg(self.text_dim)
        } else {
            Style::default().add_modifier(Modifier::DIM)
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::new(Palette::default(), Tier::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_thresholds_are_inclusive_at_the_boundary() {
        let t = Theme::new(Palette::Classic, Tier::Ansi16);
        assert_eq!(t.heat(0.0), t.ok);
        assert_eq!(t.heat(49.9), t.ok);
        assert_eq!(t.heat(50.0), t.warn);
        assert_eq!(t.heat(79.9), t.warn);
        assert_eq!(t.heat(80.0), t.critical);
        assert_eq!(t.heat(100.0), t.critical);
    }

    #[test]
    fn heat_handles_values_outside_the_nominal_range() {
        // Per-process CPU exceeds 100 on threaded processes, and a NaN would
        // fall through every comparison arm.
        let t = Theme::new(Palette::Classic, Tier::Ansi16);
        assert_eq!(t.heat(400.0), t.critical);
        assert_eq!(t.heat(-1.0), t.ok);
        assert_eq!(t.heat(f32::NAN), t.ok);
    }

    #[test]
    fn mono_tier_names_no_colour_at_all() {
        let t = Theme::new(Palette::Classic, Tier::Mono);
        for c in [
            t.ok,
            t.warn,
            t.critical,
            t.series_cpu,
            t.series_mem,
            t.chrome,
            t.text,
            t.text_dim,
            t.selection_bg,
            t.live,
        ] {
            assert_eq!(c, Color::Reset, "mono tier must not name a colour");
        }
    }

    #[test]
    fn mono_still_separates_every_severity() {
        // The point of the tier: without colour, the three states must still be
        // three distinct styles, or meaning is carried by colour alone.
        let t = Theme::new(Palette::Classic, Tier::Mono);
        let ok = t.heat_style(10.0);
        let warn = t.heat_style(60.0);
        let crit = t.heat_style(90.0);
        assert_ne!(ok, warn);
        assert_ne!(warn, crit);
        assert_ne!(ok, crit);
    }

    #[test]
    fn mono_severity_ordering_survives_greyscale() {
        // Heavier means worse. The hues cannot do this — yellow is lighter than
        // green and red darker than both — so the modifier ordering carries it.
        let t = Theme::new(Palette::Classic, Tier::Mono);
        assert!(t.heat_style(90.0).add_modifier.contains(Modifier::BOLD));
        assert!(t.heat_style(10.0).add_modifier.contains(Modifier::DIM));
    }

    #[test]
    fn mono_figure_style_keeps_all_three_severities_apart() {
        // The bug this guards: composing heat_style with an unconditional bold
        // made warn and critical identical without colour.
        let t = Theme::new(Palette::Classic, Tier::Mono);
        let styles = [
            t.figure_style(10.0),
            t.figure_style(60.0),
            t.figure_style(90.0),
        ];
        for (i, a) in styles.iter().enumerate() {
            for b in &styles[i + 1..] {
                assert_ne!(a, b, "two severities render identically at mono");
            }
        }
    }

    #[test]
    fn coloured_figures_are_bold_and_hued() {
        let t = Theme::new(Palette::Classic, Tier::TrueColor);
        let s = t.figure_style(90.0);
        assert!(s.add_modifier.contains(Modifier::BOLD));
        assert_eq!(s.fg, Some(t.critical));
    }

    #[test]
    fn mono_keeps_paused_and_selection_distinguishable() {
        let t = Theme::new(Palette::Classic, Tier::Mono);
        assert!(
            t.paused_style().add_modifier.contains(Modifier::REVERSED),
            "paused must stay loud without colour"
        );
        assert!(
            t.selection_style()
                .add_modifier
                .contains(Modifier::REVERSED),
            "selection must stay visible without colour"
        );
        assert_ne!(t.selection_style(), Style::default());
    }

    #[test]
    fn attention_affordances_use_reverse_video_at_every_tier() {
        // Not a colour pair: white-on-something vanishes on a light terminal,
        // and an ANSI slot is whatever the user's theme says it is.
        for palette in [Palette::Safe, Palette::Classic] {
            for tier in [Tier::Mono, Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
                let t = Theme::new(palette, tier);
                assert!(
                    t.paused_style().add_modifier.contains(Modifier::REVERSED),
                    "{palette:?}/{tier:?}: PAUSED badge is not reverse video"
                );
                assert!(
                    t.cursor_style().add_modifier.contains(Modifier::REVERSED),
                    "{palette:?}/{tier:?}: cursor is not reverse video"
                );
            }
        }
    }

    #[test]
    fn the_256_tier_emits_indexed_colour_not_truecolor() {
        // The tier exists for terminals that cannot parse a 24-bit escape;
        // emitting Rgb here would send ESC[38;2;… to exactly those terminals.
        for palette in [Palette::Safe, Palette::Classic] {
            let t = Theme::new(palette, Tier::Ansi256);
            for c in [t.ok, t.warn, t.critical, t.chrome, t.text, t.selection_bg] {
                assert!(
                    matches!(c, Color::Indexed(_)),
                    "{palette:?} 256 tier emitted {c:?}, not indexed"
                );
            }
        }
    }

    #[test]
    fn the_256_and_truecolor_tiers_are_genuinely_different() {
        for palette in [Palette::Safe, Palette::Classic] {
            let a = Theme::new(palette, Tier::Ansi256);
            let b = Theme::new(palette, Tier::TrueColor);
            assert_ne!(a.ok, b.ok, "{palette:?}: 256 and truecolor identical");
            assert_ne!(a.critical, b.critical);
        }
    }

    #[test]
    fn selection_sets_a_foreground_with_its_background() {
        // A background alone inherits the terminal default foreground, which on
        // a light theme is dark — an invisible selected row.
        for palette in [Palette::Safe, Palette::Classic] {
            for tier in [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
                let s = Theme::new(palette, tier).selection_style();
                assert!(s.bg.is_some());
                assert!(s.fg.is_some(), "{palette:?}/{tier:?} selection inherits fg");
            }
        }
    }

    #[test]
    fn dim_uses_one_mechanism_not_both() {
        // DIM blends toward the background, so a dim hue plus the dim modifier
        // compounds into a contrast regression.
        for palette in [Palette::Safe, Palette::Classic] {
            for tier in [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
                let s = Theme::new(palette, tier).dim_style();
                assert!(s.fg.is_some());
                assert!(
                    !s.add_modifier.contains(Modifier::DIM),
                    "{palette:?}/{tier:?} double-dims"
                );
            }
            let mono = Theme::new(palette, Tier::Mono).dim_style();
            assert!(mono.add_modifier.contains(Modifier::DIM));
            assert_eq!(mono.fg, None);
        }
    }

    #[test]
    fn no_color_must_be_non_empty_to_count() {
        // `export NO_COLOR=""` is how a profile opts back in to colour.
        assert_eq!(
            Tier::detect_from(false, Some("truecolor"), Some("xterm")),
            Tier::TrueColor
        );
        assert_eq!(
            Tier::detect_from(true, Some("truecolor"), Some("xterm")),
            Tier::Mono
        );
    }

    #[test]
    fn rich_tiers_control_their_own_hues() {
        // ANSI slots are chosen by the user's terminal theme; RGB is not.
        for palette in [Palette::Safe, Palette::Classic] {
            let t = Theme::new(palette, Tier::TrueColor);
            assert!(matches!(t.ok, Color::Rgb(..)));
            assert!(matches!(t.critical, Color::Rgb(..)));
        }
    }

    #[test]
    fn tier_detection_rules() {
        use Tier::*;
        assert_eq!(
            Tier::detect_from(false, Some("truecolor"), Some("xterm")),
            TrueColor
        );
        assert_eq!(
            Tier::detect_from(false, Some("24bit"), Some("xterm")),
            TrueColor
        );
        assert_eq!(
            Tier::detect_from(false, None, Some("xterm-256color")),
            Ansi256
        );
        assert_eq!(Tier::detect_from(false, None, Some("xterm")), Ansi16);
        assert_eq!(Tier::detect_from(false, None, Some("linux")), Ansi16);
        // NO_COLOR wins over everything, including truecolor.
        assert_eq!(
            Tier::detect_from(true, Some("truecolor"), Some("xterm-256color")),
            Mono
        );
        // A dumb or absent terminal gets no colour.
        assert_eq!(Tier::detect_from(false, None, Some("dumb")), Mono);
        assert_eq!(Tier::detect_from(false, None, None), Mono);
    }

    /// The five colours that carry meaning, in a fixed order.
    fn meaning_bearing(t: &Theme) -> [(&'static str, Color); 5] {
        [
            ("ok", t.ok),
            ("warn", t.warn),
            ("critical", t.critical),
            ("series_cpu", t.series_cpu),
            ("series_mem", t.series_mem),
        ]
    }

    #[test]
    fn the_safe_palette_meets_its_separation_target() {
        // This is the claim in `safe_truecolor`'s doc comment, enforced. A hex
        // nudged by eye fails the build rather than a reviewer's judgement.
        //
        // Asserted through `check::Report` — the same instrument
        // `--check-theme` prints. A separate copy of the arithmetic here would
        // let CI and the user's own check disagree about the same palette,
        // which would make the outward-facing one worthless.
        for tier in [Tier::Ansi256, Tier::TrueColor] {
            let report = crate::check::Report::of("safe", &Theme::new(Palette::Safe, tier));
            assert!(report.passes(), "{tier:?}:\n{report}");
        }
    }

    #[test]
    fn the_safe_palette_is_legible_on_every_background_it_is_drawn_over() {
        // Separation between hues says nothing about whether a hue is visible
        // at all. The first 256-colour attempt cleared ΔE comfortably while
        // sitting at 2.03:1 on the selected row.
        //
        // Both backgrounds are in the report, so this is the same assertion as
        // above viewed from the other side; kept separate because the two
        // failures mean different things and deserve different names.
        for tier in [Tier::Ansi256, Tier::TrueColor] {
            let report = crate::check::Report::of("safe", &Theme::new(Palette::Safe, tier));
            for l in &report.legibility {
                assert!(
                    l.passes(),
                    "{tier:?}: {} is {:.2}:1 against the {}, below {}:1",
                    l.token,
                    l.ratio,
                    l.background,
                    crate::check::MIN_CONTRAST
                );
            }
        }
    }

    #[test]
    fn the_classic_palette_is_knowingly_below_the_target() {
        // Not a bug: classic exists to restore the convention, and the whole
        // reason `safe` is the default is that this pair is indistinguishable.
        // Pinned so that "improving" classic is a deliberate act, not a silent
        // one that removes the argument for the default.
        let report =
            crate::check::Report::of("classic", &Theme::new(Palette::Classic, Tier::TrueColor));
        assert!(
            !report.passes(),
            "classic now passes its own check; the case for `safe` being the \
             default has changed and the docs need revisiting:\n{report}"
        );
        let worst = report.worst_pair().expect("a palette has pairs");
        assert_eq!(
            (worst.a, worst.b),
            ("ok", "warn"),
            "classic's worst pair is no longer green/yellow, which is the pair \
             the whole argument is about"
        );
        // …and the caveat has to be the thing `--check-theme` shows a user, or
        // ptop reads as failing its own check.
        assert!(Palette::Classic.caveat().is_some());
    }

    #[test]
    fn the_ansi16_tier_is_unmeasurable_by_construction() {
        // Stated rather than skipped: the user's terminal theme picks these,
        // so there is no value to check. It is why the richer tiers exist.
        use crate::cvd::to_rgb;
        for palette in [Palette::Safe, Palette::Classic] {
            let th = Theme::new(palette, Tier::Ansi16);
            for (name, c) in meaning_bearing(&th) {
                assert_eq!(
                    to_rgb(c),
                    None,
                    "{palette:?} ansi16 {name} claims a measurable value"
                );
            }
        }
    }

    #[test]
    fn chrome_and_title_never_collapse_onto_one_slot() {
        // The hierarchy has to exist at every colour tier, including the one
        // where it cannot be measured — ANSI slots have no value to compare, so
        // distinctness is all that can be asserted there.
        for palette in [Palette::Safe, Palette::Classic] {
            for tier in [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
                let th = Theme::new(palette, tier);
                assert_ne!(
                    th.chrome, th.text_dim,
                    "{palette:?}/{tier:?}: border and title share a slot"
                );
                assert_ne!(th.text_dim, th.text);
            }
        }
    }

    #[test]
    fn chrome_recedes_behind_the_data() {
        // A visual hierarchy, asserted: border < title < data. Without this the
        // box drawing competes with the numbers inside it.
        use crate::cvd::{contrast, to_rgb};
        let surface = [0x1a, 0x1a, 0x19];
        for palette in [Palette::Safe, Palette::Classic] {
            let th = Theme::new(palette, Tier::TrueColor);
            let c = |col| contrast(to_rgb(col).unwrap(), surface);
            let (chrome, dim, data) = (c(th.chrome), c(th.text_dim), c(th.ok));
            assert!(
                chrome < dim && dim < data,
                "{palette:?}: chrome {chrome:.2} / dim {dim:.2} / data {data:.2} \
                 are not in recessive order"
            );
            // Recessive, but not invisible — a border nobody can find is worse
            // than no border.
            assert!(
                chrome >= 1.5,
                "{palette:?}: chrome at {chrome:.2}:1 is too faint"
            );
        }
    }

    #[test]
    fn chrome_survives_the_mono_tier() {
        let th = Theme::new(Palette::Safe, Tier::Mono);
        assert!(th.chrome_style().add_modifier.contains(Modifier::DIM));
        assert_eq!(th.chrome_style().fg, None);
    }

    #[test]
    fn no_palette_reuses_a_status_hue_for_identity() {
        // Status means a state; identity means which thing this is. A series
        // that borrows a status hue destroys the status hue's meaning.
        //
        // Looped over both palettes deliberately: testing only Safe let G5 ship
        // a classic timeline drawing CPU and MEM in one hue — the `ok` hue, so
        // a machine at 95% drew its timeline in green.
        for (palette, tier) in [Palette::Safe, Palette::Classic]
            .into_iter()
            .flat_map(|p| [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor].map(|t| (p, t)))
        {
            let t = Theme::new(palette, tier);
            for status in [t.ok, t.warn, t.critical] {
                assert_ne!(
                    t.series_cpu, status,
                    "{palette:?}/{tier:?}: cpu borrows a status hue"
                );
                assert_ne!(
                    t.series_mem, status,
                    "{palette:?}/{tier:?}: mem borrows a status hue"
                );
            }
            assert_ne!(
                t.series_cpu, t.series_mem,
                "{palette:?}/{tier:?}: series identical"
            );
        }
    }

    #[test]
    fn identity_hues_clear_both_status_palettes() {
        // The series hues are shared, so they must separate from *both* status
        // sets — the classic one included, even though its own status hues are
        // knowingly below target.
        use crate::cvd::{CVD_TARGET, to_rgb, worst_cvd};
        for tier in [Tier::Ansi256, Tier::TrueColor] {
            let series = Theme::new(Palette::Safe, tier);
            let (cpu, mem) = (
                to_rgb(series.series_cpu).unwrap(),
                to_rgb(series.series_mem).unwrap(),
            );
            assert!(worst_cvd(cpu, mem).0 >= CVD_TARGET);
            for palette in [Palette::Safe, Palette::Classic] {
                let th = Theme::new(palette, tier);
                assert_eq!(th.series_cpu, series.series_cpu, "identity hues diverged");
                assert_eq!(th.series_mem, series.series_mem, "identity hues diverged");
                for (name, s) in [("ok", th.ok), ("warn", th.warn), ("crit", th.critical)] {
                    let s = to_rgb(s).unwrap();
                    for (sn, sv) in [("cpu", cpu), ("mem", mem)] {
                        let d = worst_cvd(sv, s).0;
                        assert!(
                            d >= CVD_TARGET,
                            "{palette:?}/{tier:?}: {sn} vs {name} is dE {d:.1}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn safe_palette_drops_green() {
        // The whole point: green↔yellow measured 3.7 apart under protanopia.
        let t = Theme::new(Palette::Safe, Tier::Ansi16);
        assert_ne!(t.ok, Color::Green);
        assert_eq!(t.ok, Color::Cyan);
    }

    #[test]
    fn classic_palette_is_still_reachable() {
        let t = Theme::new(Palette::Classic, Tier::Ansi16);
        assert_eq!(t.ok, Color::Green);
        assert_eq!(t.warn, Color::Yellow);
        assert_eq!(t.critical, Color::Red);
    }

    #[test]
    fn mono_ignores_the_palette_entirely() {
        assert_eq!(
            Theme::new(Palette::Safe, Tier::Mono),
            Theme::new(Palette::Classic, Tier::Mono)
        );
    }

    #[test]
    fn palette_parse_round_trips() {
        assert_eq!(Palette::parse("safe"), Some(Palette::Safe));
        assert_eq!(Palette::parse("classic"), Some(Palette::Classic));
        assert_eq!(Palette::parse("nonsense"), None);
        assert_eq!(Palette::default(), Palette::Safe);
    }

    #[test]
    fn tier_parse_round_trips() {
        assert_eq!(Tier::parse("mono"), Some(Tier::Mono));
        assert_eq!(Tier::parse("16"), Some(Tier::Ansi16));
        assert_eq!(Tier::parse("256"), Some(Tier::Ansi256));
        assert_eq!(Tier::parse("true"), Some(Tier::TrueColor));
        assert_eq!(Tier::parse("nonsense"), None);
    }
}

/// The themeable tokens.
///
/// Exactly the vocabulary the palette was built around: a status is a state,
/// a series is an identity, and chrome is neither. Nothing new is invented for
/// user themes — a theme that could name colours the code does not use would
/// be a second vocabulary to keep in step with the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Ok,
    Warn,
    Critical,
    SeriesCpu,
    SeriesMem,
    Chrome,
    Text,
    TextDim,
    SelectionBg,
    Live,
}

impl Token {
    /// Every token, in the order a theme file should list them.
    pub const ALL: [Token; 10] = [
        Token::Ok,
        Token::Warn,
        Token::Critical,
        Token::SeriesCpu,
        Token::SeriesMem,
        Token::Chrome,
        Token::Text,
        Token::TextDim,
        Token::SelectionBg,
        Token::Live,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Token::Ok => "ok",
            Token::Warn => "warn",
            Token::Critical => "critical",
            Token::SeriesCpu => "series_cpu",
            Token::SeriesMem => "series_mem",
            Token::Chrome => "chrome",
            Token::Text => "text",
            Token::TextDim => "text_dim",
            Token::SelectionBg => "selection_bg",
            Token::Live => "live",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == s)
    }

    pub fn get(self, theme: &Theme) -> Color {
        match self {
            Token::Ok => theme.ok,
            Token::Warn => theme.warn,
            Token::Critical => theme.critical,
            Token::SeriesCpu => theme.series_cpu,
            Token::SeriesMem => theme.series_mem,
            Token::Chrome => theme.chrome,
            Token::Text => theme.text,
            Token::TextDim => theme.text_dim,
            Token::SelectionBg => theme.selection_bg,
            Token::Live => theme.live,
        }
    }

    fn set(self, theme: &mut Theme, c: Color) {
        match self {
            Token::Ok => theme.ok = c,
            Token::Warn => theme.warn = c,
            Token::Critical => theme.critical = c,
            Token::SeriesCpu => theme.series_cpu = c,
            Token::SeriesMem => theme.series_mem = c,
            Token::Chrome => theme.chrome = c,
            Token::Text => theme.text = c,
            Token::TextDim => theme.text_dim = c,
            Token::SelectionBg => theme.selection_bg = c,
            Token::Live => theme.live = c,
        }
    }
}

/// A colour written three ways: `#5ccfe6`, a 256-colour index, or an ANSI name.
///
/// All three, because all three are the right answer at some tier. Hex is what
/// a designer hands you; an index is what someone matching a 256-colour scheme
/// has; a name is the only thing that means anything on a 16-colour terminal,
/// where the actual hue belongs to the user's terminal theme and not to ptop.
pub fn parse_color(s: &str) -> Option<Color> {
    if let Some(hex) = s.strip_prefix('#') {
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?));
    }
    if let Ok(n) = s.parse::<u8>() {
        // Only when the whole string is the number: `80x` is a typo, not 80.
        if s.chars().all(|c| c.is_ascii_digit()) {
            return Some(Color::Indexed(n));
        }
    }
    Some(match s {
        "black" => Color::Black,
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        "gray" | "grey" => Color::Gray,
        "darkgray" | "darkgrey" => Color::DarkGray,
        "lightred" => Color::LightRed,
        "lightgreen" => Color::LightGreen,
        "lightyellow" => Color::LightYellow,
        "lightblue" => Color::LightBlue,
        "lightmagenta" => Color::LightMagenta,
        "lightcyan" => Color::LightCyan,
        "white" => Color::White,
        // The terminal's own foreground. The only way to say "leave this one
        // alone" in a file whose every other value picks something.
        "default" | "reset" => Color::Reset,
        _ => return None,
    })
}

/// How a colour is written back out, so a theme file round-trips.
pub fn write_color(c: Color) -> String {
    match c {
        Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
        Color::Indexed(n) => n.to_string(),
        Color::Reset => "default".to_string(),
        other => format!("{other:?}").to_lowercase(),
    }
}

impl Tier {
    /// The lowest tier that can show a colour as written.
    ///
    /// There is no quantisation here on purpose. Squeezing 24-bit hex into
    /// sixteen slots would destroy exactly the separation the palettes were
    /// measured for — and the sixteen slots do not belong to ptop anyway, they
    /// belong to the user's terminal theme. A colour the tier cannot show is
    /// therefore left alone and reported, not approximated.
    pub fn needed_for(c: Color) -> Tier {
        match c {
            Color::Rgb(..) => Tier::TrueColor,
            // Indices 0-15 *are* the ANSI slots: `ok = 6` and `ok = cyan` name
            // the same thing, and a 16-colour terminal renders both. Only the
            // cube above them needs a 256-colour terminal.
            Color::Indexed(0..=15) => Tier::Ansi16,
            Color::Indexed(..) => Tier::Ansi256,
            _ => Tier::Ansi16,
        }
    }
}

impl Theme {
    /// This theme with a user's tokens written over it.
    ///
    /// Returns the tokens it could not apply, which are the ones this terminal
    /// cannot show as written. Silently dropping them would leave a user
    /// staring at the built-in palette wondering why their file did nothing.
    pub fn with_overrides(mut self, overrides: &[(Token, Color)]) -> (Self, Vec<Token>) {
        // Monochrome ignores palettes entirely, which is the clearest possible
        // statement that meaning never rests on colour here. A user theme does
        // not get to weaken that.
        if self.tier == Tier::Mono {
            return (self, overrides.iter().map(|&(t, _)| t).collect());
        }
        let mut skipped = Vec::new();
        for &(token, colour) in overrides {
            if Tier::needed_for(colour) > self.tier {
                skipped.push(token);
            } else {
                token.set(&mut self, colour);
            }
        }
        (self, skipped)
    }
}
