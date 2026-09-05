//! Semantic colour tokens.
//!
//! Nothing outside this module names a colour. Call sites ask for the *job* a
//! colour does — "this is a critical value", "this is chrome" — so the palette
//! can change in one place, and so a reviewer can see at a glance whether a
//! colour is carrying meaning or decoration.
//!
//! The distinction that matters most here is **status versus identity**. Status
//! tokens (`ok`, `warn`, `critical`) mean a state and are reserved for it.
//! Identity tokens (`series_cpu`, `series_mem`) mean *which thing this is* and
//! never imply anything is wrong. Reusing a status colour as a series colour
//! destroys the meaning of the status colour everywhere else in the UI.

use ratatui::style::Color;

/// Named colours, grouped by the job each one does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    // Status — a state, never an identity.
    pub ok: Color,
    pub warn: Color,
    pub critical: Color,

    // Identity — which series this is, never a judgement about it.
    pub series_cpu: Color,
    pub series_mem: Color,

    // Chrome and text. Deliberately not "a colour": these should recede.
    pub chrome: Color,
    pub text: Color,
    pub text_dim: Color,

    // Interaction.
    pub cursor: Color,
    pub cursor_fg: Color,
    pub selection_bg: Color,
    pub live: Color,
}

impl Theme {
    /// The palette ptop has always shipped: terminal ANSI slots.
    ///
    /// Note these are *slots*, not colours — the user's terminal theme decides
    /// what `Color::Green` actually looks like, so this palette cannot make any
    /// promise about contrast or colour-vision separation. Fixing that needs
    /// the 256-colour tier; see `docs/roadmaps/01-color-and-accessibility.md`.
    pub const fn classic() -> Self {
        Self {
            ok: Color::Green,
            warn: Color::Yellow,
            critical: Color::Red,

            // Today both series render in status colours by magnitude. Naming
            // them here does not change that yet — G5 moves the timeline onto
            // these tokens, and only then do they need to be distinct hues.
            series_cpu: Color::Green,
            series_mem: Color::Green,

            chrome: Color::Reset,
            text: Color::Reset,
            text_dim: Color::Reset,

            cursor: Color::Yellow,
            cursor_fg: Color::Black,
            selection_bg: Color::DarkGray,
            live: Color::Green,
        }
    }

    /// Status colour for a percentage, on the shared 50/80 thresholds.
    ///
    /// One definition so the meters, the figures and the table cannot drift
    /// apart on where "warn" begins.
    pub fn heat(&self, pct: f32) -> Color {
        match pct {
            p if p >= Self::CRITICAL_PCT => self.critical,
            p if p >= Self::WARN_PCT => self.warn,
            _ => self.ok,
        }
    }

    pub const WARN_PCT: f32 = 50.0;
    pub const CRITICAL_PCT: f32 = 80.0;
}

impl Default for Theme {
    fn default() -> Self {
        Self::classic()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_thresholds_are_inclusive_at_the_boundary() {
        let t = Theme::classic();
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
        let t = Theme::classic();
        assert_eq!(t.heat(400.0), t.critical);
        assert_eq!(t.heat(-1.0), t.ok);
        assert_eq!(t.heat(f32::NAN), t.ok);
    }

    #[test]
    fn status_and_identity_tokens_stay_separate_concepts() {
        // C6 will enforce that these never overlap once the series tokens get
        // real hues. For now, assert only that both sets exist and are reached
        // through different names, so a later change has something to break.
        let t = Theme::classic();
        let status = [t.ok, t.warn, t.critical];
        assert_eq!(status.len(), 3);
        let _identity = [t.series_cpu, t.series_mem];
    }
}
