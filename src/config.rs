//! `key = value` configuration.
//!
//! Hand-rolled rather than TOML. ptop's config surface is genuinely flat, and
//! `serde` + `toml` would be the largest dependency in the project by an order
//! of magnitude — in a codebase whose `/proc` parser is deliberately hand-rolled
//! with no dependencies at all. htop and btop both use `key = value` and neither
//! has outgrown it. If real nesting ever appears that is the moment to
//! reconsider, and not before.
//!
//! One table, [`KEYS`], defines every setting exactly once. The config file and
//! the command line both drive it, so `theme = classic` and `--theme=classic`
//! cannot come to disagree about what a value means, and a setting added for
//! one gets the other for free.

use crate::glyphs::GlyphSet;
use crate::theme::{Palette, Tier};
use std::ffi::OsString;
use std::path::PathBuf;

/// Everything settable, resolved.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Settings {
    pub glyphs: GlyphSet,
    pub tier: Tier,
    pub palette: Palette,
}

impl Settings {
    /// The built-in defaults, which are partly a question about the terminal:
    /// a Linux console has no braille glyphs and `NO_COLOR` means what it says.
    pub fn detect() -> Self {
        Self {
            glyphs: default_glyphs(),
            tier: Tier::detect(),
            palette: Palette::default(),
        }
    }
}

/// Braille unless we are on a real Linux console, whose font has no braille
/// glyphs. btop makes the same check (`btop.cpp:815`).
fn default_glyphs() -> GlyphSet {
    default_glyphs_for(std::env::var("TERM").ok().as_deref())
}

/// The environment split out, so the rule is testable. See [`path_from`].
fn default_glyphs_for(term: Option<&str>) -> GlyphSet {
    match term {
        Some("linux") => GlyphSet::Ascii,
        _ => GlyphSet::Braille,
    }
}

/// How to apply one value. Returns what was expected, for the error message.
type Apply = fn(&mut Settings, &str) -> Result<(), &'static str>;

/// Every setting, named once.
pub const KEYS: &[(&str, Apply)] = &[
    ("glyphs", |s, v| {
        s.glyphs = GlyphSet::parse(v).ok_or("braille, block or ascii")?;
        Ok(())
    }),
    ("color", |s, v| {
        // Re-detect rather than no-op, so a later `--color=auto` can override
        // an earlier one baked into a wrapper script or the config file.
        s.tier = match v {
            "auto" => Tier::detect(),
            _ => Tier::parse(v).ok_or("auto, mono, 16, 256 or true")?,
        };
        Ok(())
    }),
    ("theme", |s, v| {
        s.palette = match v {
            "auto" | "default" => Palette::default(),
            _ => Palette::parse(v).ok_or("safe, classic or auto")?,
        };
        Ok(())
    }),
];

/// Why a setting could not be applied.
#[derive(Debug)]
pub enum Bad {
    UnknownKey {
        key: String,
        hint: Option<&'static str>,
    },
    Value {
        key: String,
        value: String,
        expected: &'static str,
    },
}

impl std::fmt::Display for Bad {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKey { key, hint } => {
                write!(f, "unknown key `{key}`")?;
                match hint {
                    Some(k) => write!(f, " (did you mean `{k}`?)"),
                    None => Ok(()),
                }
            }
            Self::Value {
                key,
                value,
                expected,
            } => {
                write!(f, "`{key}`: expected {expected}, found `{value}`")
            }
        }
    }
}

impl Bad {
    /// The same complaint, spelled the way the command line spells settings.
    ///
    /// `--glyphs=crayon: expected braille, block or ascii` rather than the
    /// file's `` `glyphs`: expected … ``. Prefixing the file form with `--`
    /// produced ``--`glyphs`: expected …``, which is neither.
    pub fn as_flag(&self) -> String {
        match self {
            Self::UnknownKey { .. } => self.to_string(),
            Self::Value {
                key,
                value,
                expected,
            } => format!("--{key}={value}: expected {expected}"),
        }
    }
}

/// Apply one setting, wherever it came from.
///
/// The single entry point for both the config file and the command line, so
/// `theme = classic` and `--theme=classic` cannot come to disagree about what
/// the value means or which values are legal.
pub fn apply(settings: &mut Settings, key: &str, value: &str) -> Result<(), Bad> {
    match KEYS.iter().find(|(k, _)| *k == key) {
        Some((_, f)) => f(settings, value).map_err(|expected| Bad::Value {
            key: key.to_string(),
            value: value.to_string(),
            expected,
        }),
        None => Err(Bad::UnknownKey {
            key: key.to_string(),
            hint: closest(key),
        }),
    }
}

/// Something ptop could not use, and where it came from.
///
/// Carried rather than printed at the point of discovery: config is read before
/// the alternate screen opens, and anything written to the terminal then is
/// erased by it. See `main::flush`.
pub struct Warning(pub String);

impl std::fmt::Display for Warning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The config file's location, honouring `XDG_CONFIG_HOME`.
///
/// A relative `XDG_CONFIG_HOME` is ignored rather than resolved, as the XDG
/// spec requires: treating it as relative to the working directory would make
/// ptop read a different config depending on where it was launched from.
pub fn path() -> Option<PathBuf> {
    path_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

/// The environment split out, so the rule can be tested without mutating a
/// process-global that every other test is reading at the same time. The same
/// shape as `Tier::detect_from`, for the same reason.
pub fn path_from(xdg: Option<OsString>, home: Option<OsString>) -> Option<PathBuf> {
    let base = xdg
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            // Relative here is the same hazard, and it happens in stripped-down
            // containers and under `env -i`: ptop would read a different file
            // depending on the directory it was launched from.
            home.map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|h| h.join(".config"))
        })?;
    Some(base.join("ptop").join("ptop.conf"))
}

/// Resolve every source into one set of settings, lowest precedence first:
/// built-in default, config file, environment, command line.
///
/// Takes its inputs rather than reading them, so the precedence rule can be
/// tested without a filesystem or a process-global environment.
///
/// A bad line in the file warns; a bad flag is fatal. The asymmetry is the
/// point: a config file is written once and read every run, so one typo must
/// not cost you the tool — but a flag was typed for this run, and quietly
/// ignoring it would do something other than what was asked.
pub fn resolve(
    mut settings: Settings,
    file: Option<(&str, &str)>,
    no_color: bool,
    args: &[String],
) -> Result<(Settings, Vec<String>, Vec<Warning>), Bad> {
    let mut warnings = Vec::new();
    if let Some((origin, text)) = file {
        apply_file(&mut settings, text, origin, &mut warnings);
    }
    // NO_COLOR outranks the file. The file records a preference in general;
    // the environment is the user saying something about this terminal now.
    if no_color {
        settings.tier = Tier::Mono;
    }

    let mut positional = Vec::new();
    for a in args {
        // Flags are the same settings the file takes, so one table serves both
        // and neither can drift. A `--flag=` naming no setting falls through
        // to be reported as an unrecognised option, which is what it is.
        match a.strip_prefix("--").and_then(|f| f.split_once('=')) {
            Some((key, value)) if KEYS.iter().any(|(k, _)| *k == key) => {
                apply(&mut settings, key, value)?;
            }
            _ => positional.push(a.clone()),
        }
    }
    Ok((settings, positional, warnings))
}

/// Read the user's config file, if there is one.
///
/// A missing file is not an error — the overwhelmingly common case is not
/// having one — and neither is an unreadable one, which is worth a word rather
/// than a refusal to start.
pub fn read(warnings: &mut Vec<Warning>) -> Option<(String, String)> {
    let path = path()?;
    match std::fs::read_to_string(&path) {
        Ok(text) => Some((path.display().to_string(), text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            warnings.push(Warning(format!("{}: {e}", path.display())));
            None
        }
    }
}

/// Apply every line of a config file, reporting what could not be applied.
///
/// A file never fails as a whole. One typo must not cost you the other nineteen
/// lines: a config that refuses to load because of a single bad key is worse
/// than one that ignores it and says so.
pub fn apply_file(settings: &mut Settings, text: &str, origin: &str, warnings: &mut Vec<Warning>) {
    for (n, raw) in text.lines().enumerate() {
        let line = n + 1;
        let mut warn = |msg: String| warnings.push(Warning(format!("{origin}:{line}: {msg}")));

        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            warn(format!("expected `key = value`, found `{trimmed}`"));
            continue;
        };
        let key = key.trim();
        let value = strip_comment(value.trim());
        if key.is_empty() {
            warn(format!("expected `key = value`, found `{trimmed}`"));
            continue;
        }
        if value.is_empty() {
            warn(format!("`{key}` has no value"));
            continue;
        }
        if let Err(bad) = apply(settings, key, value) {
            warn(bad.to_string());
        }
    }
}

/// Drop a trailing `# comment` from a value.
///
/// A `#` that *opens* the value is kept, because that is a colour: `ok =
/// #5ccfe6 # nord blue` sets `#5ccfe6` and comments the rest. Requiring
/// whitespace before the marker is what makes the two tell apart without a
/// special case for hex.
fn strip_comment(value: &str) -> &str {
    // …and only if it is one token. `#5ccfe6` is a colour; `# not set yet` is
    // a comment on a line whose value is missing, and saying *that* beats
    // reporting a malformed colour on a line that never named one.
    let opens_value = value.starts_with('#') && !value[1..].starts_with(char::is_whitespace);
    match value.char_indices().find(|&(i, c)| {
        c == '#' && (i > 0 || !opens_value) && (i == 0 || value[..i].ends_with(char::is_whitespace))
    }) {
        Some((i, _)) => value[..i].trim_end(),
        None => value,
    }
}

/// The nearest known key, for `unknown key \`colour\`` → ``did you mean `color`?``.
///
/// Bounded by half the key's length so a genuinely unrelated word gets no
/// suggestion — a confidently wrong hint is worse than none.
fn closest(key: &str) -> Option<&'static str> {
    KEYS.iter()
        .map(|&(k, _)| (distance(key, k), k))
        .filter(|&(d, k)| d <= k.len() / 2 + 1)
        .min()
        .map(|(_, k)| k)
}

/// Levenshtein distance, two rows at a time.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0; b.len() + 1];
    for (i, ca) in a.chars().enumerate() {
        cur[0] = i + 1;
        for (j, &cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Apply a config body to freshly detected settings, returning both the
    /// result and every complaint.
    fn apply(text: &str) -> (Settings, Vec<String>) {
        let mut s = Settings {
            glyphs: GlyphSet::Braille,
            tier: Tier::TrueColor,
            palette: Palette::Safe,
        };
        let mut w = Vec::new();
        apply_file(&mut s, text, "conf", &mut w);
        (s, w.into_iter().map(|x| x.0).collect())
    }

    #[test]
    fn every_flag_is_settable_from_the_file() {
        // The point of the shared table: a setting the command line can reach
        // and the file cannot is the drift this design exists to prevent.
        let (s, w) = apply("glyphs = block\ncolor = mono\ntheme = classic\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(
            s,
            Settings {
                glyphs: GlyphSet::Block,
                tier: Tier::Mono,
                palette: Palette::Classic,
            }
        );
    }

    #[test]
    fn comments_and_blank_lines_are_not_settings() {
        let (s, w) = apply("# a comment\n\n   \nglyphs = ascii   # trailing\n");
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(s.glyphs, GlyphSet::Ascii);
    }

    #[test]
    fn a_value_may_begin_with_a_hash() {
        // Colours are written `#5ccfe6`, and the comment marker is the same
        // character. Item 0002 depends on this parser getting it right.
        assert_eq!(strip_comment("#5ccfe6"), "#5ccfe6");
        assert_eq!(strip_comment("#5ccfe6 # nord blue"), "#5ccfe6");
        assert_eq!(strip_comment("safe # my pick"), "safe");
        assert_eq!(strip_comment("safe#notacomment"), "safe#notacomment");
    }

    #[test]
    fn one_bad_line_does_not_cost_the_others() {
        // A config that refuses to load over a single typo is worse than one
        // that ignores it and says so.
        //
        // Every kind of bad line, because they leave the loop by different
        // routes and an abort on any one of them would strand the settings
        // after it.
        let (s, w) = apply(
            "glyphs = block\n\
             nonsense\n\
             colour = mono\n\
             theme = nosuchtheme\n\
             color =\n\
             theme = classic\n",
        );
        assert_eq!(
            s.glyphs,
            GlyphSet::Block,
            "a line before the bad ones was lost"
        );
        assert_eq!(s.palette, Palette::Classic, "a line after them was lost");
        assert_eq!(w.len(), 4, "{w:?}");
        for (i, line) in (2..=5).enumerate() {
            assert!(w[i].starts_with(&format!("conf:{line}:")), "{}", w[i]);
        }
    }

    #[test]
    fn an_unknown_key_names_itself_its_line_and_a_near_miss() {
        let (_, w) = apply("\n\ncolour = mono\n");
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("conf:3:"), "no line number: {}", w[0]);
        assert!(w[0].contains("`colour`"), "no key: {}", w[0]);
        assert!(w[0].contains("`color`"), "no suggestion: {}", w[0]);
    }

    #[test]
    fn a_wrong_value_says_what_was_expected() {
        let (s, w) = apply("glyphs = crayon\n");
        assert_eq!(s.glyphs, GlyphSet::Braille, "a rejected value was applied");
        assert_eq!(w.len(), 1);
        assert!(
            w[0].contains("crayon") && w[0].contains("braille"),
            "{}",
            w[0]
        );
    }

    #[test]
    fn a_wild_guess_gets_no_suggestion() {
        // A confidently wrong hint is worse than none.
        assert_eq!(closest("supercalifragilistic"), None);
        assert_eq!(closest("colour"), Some("color"));
        assert_eq!(closest("theem"), Some("theme"));
    }

    #[test]
    fn a_line_missing_half_of_itself_says_so() {
        // `theme = # not set yet` used to report a malformed *colour*, because
        // the leading-`#` exemption fired on a line that never named a value.
        // The message has to describe the actual problem to be worth printing.
        let (_, w) = apply("theme = # not set yet\n= mono\n");
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("no value"), "{}", w[0]);
        assert!(w[1].contains("key = value"), "{}", w[1]);
    }

    #[test]
    fn a_flag_complaint_is_spelled_like_a_flag() {
        // Prefixing the file's rendering with `--` produced
        // ``--`glyphs`: expected …``, which is neither form.
        let bad = apply_one("glyphs", "crayon");
        assert_eq!(
            bad.as_flag(),
            "--glyphs=crayon: expected braille, block or ascii"
        );
        assert_eq!(
            bad.to_string(),
            "`glyphs`: expected braille, block or ascii, found `crayon`"
        );
    }

    fn apply_one(key: &str, value: &str) -> Bad {
        let mut s = Settings {
            glyphs: GlyphSet::Braille,
            tier: Tier::TrueColor,
            palette: Palette::Safe,
        };
        super::apply(&mut s, key, value).expect_err("value should be rejected")
    }

    #[test]
    fn a_linux_console_gets_ascii() {
        // A real console has no braille glyphs. This moved out of `main` with
        // the rest of the defaults and arrived here untested.
        assert_eq!(default_glyphs_for(Some("linux")), GlyphSet::Ascii);
        assert_eq!(
            default_glyphs_for(Some("xterm-256color")),
            GlyphSet::Braille
        );
        assert_eq!(default_glyphs_for(None), GlyphSet::Braille);
    }

    #[test]
    fn a_relative_xdg_config_home_is_ignored() {
        // The XDG spec requires it: honouring a relative path would make ptop
        // read a different config depending on where it was launched from.
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(
            path_from(os("relative/path"), os("/home/someone")),
            Some(PathBuf::from("/home/someone/.config/ptop/ptop.conf"))
        );
        assert_eq!(
            path_from(os("/xdg"), os("/home/someone")),
            Some(PathBuf::from("/xdg/ptop/ptop.conf"))
        );
        // A relative HOME is the same hazard, and happens under `env -i`.
        assert_eq!(path_from(None, os("relative")), None);
        // No HOME and no XDG is not a crash; it is simply no config file.
        assert_eq!(path_from(None, None), None);
    }
}

#[cfg(test)]
mod precedence {
    use super::*;

    fn base() -> Settings {
        Settings {
            glyphs: GlyphSet::Braille,
            tier: Tier::TrueColor,
            palette: Palette::Safe,
        }
    }

    fn run(file: Option<&str>, no_color: bool, args: &[&str]) -> (Settings, Vec<String>) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (s, positional, _) =
            resolve(base(), file.map(|t| ("conf", t)), no_color, &args).expect("flags are valid");
        (s, positional)
    }

    #[test]
    fn the_file_beats_the_built_in_default() {
        let (s, _) = run(Some("theme = classic\n"), false, &[]);
        assert_eq!(s.palette, Palette::Classic);
    }

    #[test]
    fn a_flag_beats_the_file() {
        // So a wrapper script can override a user's file without editing it.
        let (s, _) = run(Some("theme = classic\n"), false, &["--theme=safe"]);
        assert_eq!(s.palette, Palette::Safe);
    }

    #[test]
    fn no_color_beats_the_file_and_a_flag_beats_no_color() {
        // The file records a preference in general; the environment says
        // something about this terminal; the flag is about this run.
        let (s, _) = run(Some("color = true\n"), true, &[]);
        assert_eq!(s.tier, Tier::Mono, "NO_COLOR lost to the config file");

        let (s, _) = run(Some("color = true\n"), true, &["--color=256"]);
        assert_eq!(s.tier, Tier::Ansi256, "an explicit flag lost to NO_COLOR");
    }

    #[test]
    fn the_last_flag_wins() {
        let (s, _) = run(None, false, &["--glyphs=block", "--glyphs=ascii"]);
        assert_eq!(s.glyphs, GlyphSet::Ascii);
    }

    #[test]
    fn a_bad_flag_is_fatal_where_a_bad_config_line_is_not() {
        // A config file is written once and read every run, so one typo must
        // not cost you the tool. A flag was typed for this run, and ignoring
        // it would silently do something other than what was asked.
        let (s, _) = run(Some("glyphs = crayon\n"), false, &[]);
        assert_eq!(s.glyphs, GlyphSet::Braille, "a rejected value was applied");

        let args = vec!["--glyphs=crayon".to_string()];
        assert!(resolve(base(), None, false, &args).is_err());
    }

    #[test]
    fn a_flag_naming_no_setting_is_left_for_the_caller_to_reject() {
        // `--wat=1` is an unrecognised option, not a config error, and must
        // reach the usage message rather than being swallowed here.
        let (_, positional) = run(None, false, &["--wat=1", "--once"]);
        assert_eq!(positional, vec!["--wat=1", "--once"]);
    }
}
