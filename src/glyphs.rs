//! Glyph sets for the timeline.
//!
//! A cell packs two samples side by side, each at one of five levels (0 = empty
//! through 4 = full), so a set is a 25-entry table indexed `left * 5 + right`.
//! btop uses the same packing with swappable tables, which is what lets a
//! terminal that cannot draw braille fall back without touching any layout
//! code.
//!
//! Cells also stack vertically: each row covers a slice of the 0..100 range, so
//! three rows of braille give twelve distinct heights.

/// How a value is drawn when one character cell must show two samples.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum GlyphSet {
    /// Braille. Two samples per cell, four levels each. The default.
    #[default]
    Braille,
    /// Quadrant blocks. Two samples per cell, but only two levels each — wider
    /// font support than braille.
    Block,
    /// Pure ASCII. One sample per cell; the pair is merged by taking the peak.
    Ascii,
}

impl GlyphSet {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "braille" => Some(Self::Braille),
            "block" => Some(Self::Block),
            "ascii" => Some(Self::Ascii),
            _ => None,
        }
    }

    /// Samples represented by one character cell.
    pub fn samples_per_cell(self) -> usize {
        match self {
            Self::Braille | Self::Block => 2,
            Self::Ascii => 1,
        }
    }

    /// Levels per sample, `0..=4`.
    pub const LEVELS: usize = 5;

    /// The glyph for a pair of levels, each `0..=4`.
    pub fn glyph(self, left: usize, right: usize) -> char {
        let (l, r) = (left.min(4), right.min(4));
        match self {
            Self::Braille => braille(l, r),
            Self::Block => BLOCK[l * Self::LEVELS + r],
            // No sub-cell resolution: show the peak so a spike is never hidden
            // by the sample next to it.
            Self::Ascii => ASCII[l.max(r)],
        }
    }

    /// Marker showing which half of a cell the scrub cursor sits on. Without
    /// this, packing two samples per cell would halve cursor precision.
    pub fn cursor_marker(self, right_half: bool) -> char {
        match self {
            Self::Ascii => '^',
            _ if right_half => '▐',
            _ => '▌',
        }
    }
}

/// Braille cells are `U+2800` plus a dot bitmask:
///
/// ```text
///   dot1 0x01   dot4 0x08
///   dot2 0x02   dot5 0x10
///   dot3 0x04   dot6 0x20
///   dot7 0x40   dot8 0x80
/// ```
///
/// Bars fill upward from the bottom, so level N lights the lowest N dots of its
/// column. Deriving this beats transcribing a 25-entry table: the rule is one
/// line and it is impossible to get a single entry subtly wrong.
fn braille(left: usize, right: usize) -> char {
    /// Bottom-up dot order for each column.
    const LEFT: [u32; 4] = [0x40, 0x04, 0x02, 0x01];
    const RIGHT: [u32; 4] = [0x80, 0x20, 0x10, 0x08];

    let mut bits = 0;
    for dot in LEFT.iter().take(left) {
        bits |= dot;
    }
    for dot in RIGHT.iter().take(right) {
        bits |= dot;
    }
    char::from_u32(0x2800 + bits).unwrap_or(' ')
}

/// Quadrant blocks, `left * 5 + right`. Levels 1-2 are the lower half and 3-4
/// the full height, so this set carries two levels per sample rather than four.
#[rustfmt::skip]
const BLOCK: [char; 25] = [
    ' ', '▗', '▗', '▐', '▐',
    '▖', '▄', '▄', '▟', '▟',
    '▖', '▄', '▄', '▟', '▟',
    '▌', '▙', '▙', '█', '█',
    '▌', '▙', '▙', '█', '█',
];

const ASCII: [char; 5] = [' ', '.', ':', '|', '#'];

/// Split a percentage into the level `0..=4` it occupies in row `row` of a
/// `rows`-tall graph, counting rows from the top.
///
/// Each row owns a band of the 0..100 range: a value above the band fills the
/// row, below it leaves the row empty, and inside it scales across the five
/// levels.
pub fn level_in_row(pct: f32, row: usize, rows: usize) -> usize {
    let rows = rows.max(1) as f32;
    let high = 100.0 * (rows - row as f32) / rows;
    let low = 100.0 * (rows - row as f32 - 1.0) / rows;

    if pct >= high {
        4
    } else if pct <= low {
        0
    } else {
        (((pct - low) * 4.0 / (high - low)).round() as usize).clamp(1, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braille_matches_the_known_encoding() {
        // Spot-checked against btop's hand-written table (btop_draw.cpp:90).
        assert_eq!(braille(0, 0), '\u{2800}');
        assert_eq!(braille(0, 1), '⢀');
        assert_eq!(braille(1, 0), '⡀');
        assert_eq!(braille(1, 1), '⣀');
        assert_eq!(braille(2, 2), '⣤');
        assert_eq!(braille(4, 4), '⣿');
        assert_eq!(braille(4, 0), '⡇');
        assert_eq!(braille(0, 4), '⢸');
    }

    #[test]
    fn every_level_pair_is_representable() {
        for set in [GlyphSet::Braille, GlyphSet::Block, GlyphSet::Ascii] {
            for l in 0..GlyphSet::LEVELS {
                for r in 0..GlyphSet::LEVELS {
                    let _ = set.glyph(l, r);
                }
            }
        }
    }

    #[test]
    fn glyphs_grow_monotonically_with_level() {
        // A taller bar must never render as a shorter glyph, which is the way a
        // hand-written table goes wrong.
        for l in 1..GlyphSet::LEVELS {
            let prev = braille(l - 1, 0) as u32;
            assert!(braille(l, 0) as u32 > prev, "left level {l} did not grow");
            let prev = braille(0, l - 1) as u32;
            assert!(braille(0, l) as u32 > prev, "right level {l} did not grow");
        }
    }

    #[test]
    fn out_of_range_levels_saturate_rather_than_panic() {
        assert_eq!(GlyphSet::Braille.glyph(99, 99), '⣿');
        assert_eq!(GlyphSet::Block.glyph(99, 99), '█');
        assert_eq!(GlyphSet::Ascii.glyph(99, 0), '#');
    }

    #[test]
    fn ascii_shows_the_peak_of_the_pair() {
        // Merging by average would hide a spike next to an idle sample, which
        // is the one thing the timeline exists to show.
        assert_eq!(GlyphSet::Ascii.glyph(0, 4), '#');
        assert_eq!(GlyphSet::Ascii.glyph(4, 0), '#');
    }

    #[test]
    fn rows_partition_the_range() {
        // Top row of three covers ~67..100, bottom covers 0..~33.
        assert_eq!(level_in_row(100.0, 0, 3), 4);
        assert_eq!(level_in_row(0.0, 0, 3), 0);
        assert_eq!(level_in_row(100.0, 2, 3), 4);
        assert_eq!(level_in_row(0.0, 2, 3), 0);
        // A mid value fills the lower rows and partly fills its own.
        assert_eq!(level_in_row(50.0, 2, 3), 4);
        assert!((1..=4).contains(&level_in_row(50.0, 1, 3)));
        assert_eq!(level_in_row(50.0, 0, 3), 0);
    }

    #[test]
    fn single_row_spans_the_whole_range() {
        assert_eq!(level_in_row(0.0, 0, 1), 0);
        assert_eq!(level_in_row(100.0, 0, 1), 4);
        assert!((1..=4).contains(&level_in_row(50.0, 0, 1)));
    }

    #[test]
    fn parse_round_trips() {
        assert_eq!(GlyphSet::parse("braille"), Some(GlyphSet::Braille));
        assert_eq!(GlyphSet::parse("block"), Some(GlyphSet::Block));
        assert_eq!(GlyphSet::parse("ascii"), Some(GlyphSet::Ascii));
        assert_eq!(GlyphSet::parse("nonsense"), None);
    }
}
