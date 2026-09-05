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

/// How much colour the terminal can be trusted with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
    /// 24-bit. Same palette as 256, at full precision.
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
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "safe" => Some(Self::Safe),
            "classic" => Some(Self::Classic),
            _ => None,
        }
    }
}

/// Named colours, grouped by the job each one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl Theme {
    pub const WARN_PCT: f32 = 50.0;
    pub const CRITICAL_PCT: f32 = 80.0;

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
            ok: Color::Cyan,
            warn: Color::Yellow,
            critical: Color::Red,
            series_cpu: Color::LightBlue,
            series_mem: Color::Magenta,
            chrome: Color::DarkGray,
            text: Color::Reset,
            text_dim: Color::DarkGray,
            selection_bg: Color::DarkGray,
            live: Color::Cyan,
        }
    }

    /// Colour-vision-safe, quantised onto the xterm 256 cube.
    const fn safe_indexed() -> Self {
        Self {
            tier: Tier::Ansi256,
            ok: Color::Indexed(80),
            warn: Color::Indexed(222),
            critical: Color::Indexed(203),
            series_cpu: Color::Indexed(104),
            series_mem: Color::Indexed(139),
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
            ok: Color::Green,
            warn: Color::Yellow,
            critical: Color::Red,
            series_cpu: Color::Green,
            series_mem: Color::Green,
            chrome: Color::DarkGray,
            text: Color::Reset,
            text_dim: Color::DarkGray,
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
            ok: Color::Indexed(114),
            warn: Color::Indexed(179),
            critical: Color::Indexed(167),
            series_cpu: Color::Indexed(114),
            series_mem: Color::Indexed(114),
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
            ok: Color::Rgb(0x77, 0xca, 0x9b),
            warn: Color::Rgb(0xcb, 0xc0, 0x6c),
            critical: Color::Rgb(0xdc, 0x4c, 0x4c),
            // Placeholders: the timeline still colours by magnitude, so these
            // are deliberately not yet distinct hues. G5 moves the timeline
            // onto them and C3 gives them separated values — cyan and violet,
            // which measure ΔE 18.3 apart and clear every status token.
            series_cpu: Color::Rgb(0x77, 0xca, 0x9b),
            series_mem: Color::Rgb(0x77, 0xca, 0x9b),
            chrome: Color::Rgb(0x50, 0x50, 0x50),
            text: Color::Rgb(0xcc, 0xcc, 0xcc),
            text_dim: Color::Rgb(0x80, 0x80, 0x80),
            selection_bg: Color::Rgb(0x3a, 0x3a, 0x3a),
            live: Color::Rgb(0x77, 0xca, 0x9b),
        }
    }

    /// Status colour for a percentage, on the shared 50/80 thresholds.
    pub fn heat(&self, pct: f32) -> Color {
        match pct {
            p if p >= Self::CRITICAL_PCT => self.critical,
            p if p >= Self::WARN_PCT => self.warn,
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
            p if p >= Self::CRITICAL_PCT => Style::default().add_modifier(Modifier::BOLD),
            p if p >= Self::WARN_PCT => Style::default(),
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

    #[test]
    fn safe_palette_never_reuses_a_status_hue_for_identity() {
        // Status means a state; identity means which thing this is. A series
        // that borrows a status hue destroys the status hue's meaning.
        for tier in [Tier::Ansi16, Tier::Ansi256, Tier::TrueColor] {
            let t = Theme::new(Palette::Safe, tier);
            for status in [t.ok, t.warn, t.critical] {
                assert_ne!(t.series_cpu, status, "{tier:?}: cpu borrows a status hue");
                assert_ne!(t.series_mem, status, "{tier:?}: mem borrows a status hue");
            }
            assert_ne!(t.series_cpu, t.series_mem, "{tier:?}: series are identical");
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
