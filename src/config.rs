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
use crate::sample::{ProcSample, Sample};
use crate::theme::{Palette, Theme, Tier, Token};
use ratatui::style::Color;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::Duration;

/// Everything settable, resolved.
#[derive(Clone, PartialEq, Debug)]
pub struct Settings {
    pub glyphs: GlyphSet,
    pub tier: Tier,
    /// The theme asked for: a built-in, or a file in `~/.config/ptop/themes`.
    /// Whether it resolves is settled in [`resolve`], which is where the
    /// loader is — the key table cannot read files.
    pub theme: String,
    /// Where `theme` was last set, as `file:line`, when that was a file.
    ///
    /// Every other setting is judged where it is read, so the line number is
    /// still in hand. A theme name can only be judged once something can look
    /// for the file, which is after the whole config has been parsed — so this
    /// is the one key that has to carry its origin forward. Without it a bad
    /// theme is the only config error that cannot point at a line.
    pub theme_origin: Option<String>,
    /// Filled in by [`resolve`]: the built-in to start from, and the user's
    /// colours to write over it.
    pub palette: Palette,
    pub overrides: Vec<(Token, Color)>,
    /// Where "getting busy" and "in trouble" begin, as percentages.
    pub warn: f32,
    pub critical: f32,
    /// Time between samples.
    pub interval: Duration,
    /// How much history to retain, in time rather than samples. Sample count
    /// is a fact about the buffer; the span is what the user actually wants.
    pub window: Duration,
}

impl Settings {
    /// The built-in defaults, which are partly a question about the terminal:
    /// a Linux console has no braille glyphs and `NO_COLOR` means what it says.
    pub fn detect() -> Self {
        Self {
            glyphs: default_glyphs(),
            tier: Tier::detect(),
            theme: Palette::default().name().to_string(),
            theme_origin: None,
            palette: Palette::default(),
            overrides: Vec::new(),
            warn: Theme::DEFAULT_WARN_PCT,
            critical: Theme::DEFAULT_CRITICAL_PCT,
            interval: crate::app::DEFAULT_INTERVAL,
            window: DEFAULT_WINDOW,
        }
    }

    /// Samples the ring buffer needs to cover [`Settings::window`].
    ///
    /// `n` samples span `n - 1` intervals, not `n` — the first one starts the
    /// clock. Rounding up and adding that sample is what makes the buffer hold
    /// at least the window asked for rather than one interval less.
    pub fn history_len(&self) -> usize {
        let per = self.interval.as_secs_f64();
        let span = self.window.as_secs_f64();
        if per <= 0.0 {
            return 1;
        }
        (span / per).ceil().max(1.0) as usize + 1
    }
}

/// Themes for tests: only the built-ins exist unless a test says otherwise.
#[cfg(test)]
fn no_themes(name: &str) -> Result<(String, String), String> {
    Err(format!("no theme `{name}`"))
}

#[cfg(test)]
impl Settings {
    /// Settings that do not depend on the environment, for tests.
    ///
    /// One fixture rather than a literal per test module: three copies of a
    /// struct literal all have to be updated together every time a setting is
    /// added, and the compiler only tells you about it after you have written
    /// the test.
    fn fixed() -> Self {
        Self {
            glyphs: GlyphSet::Braille,
            tier: Tier::TrueColor,
            theme: Palette::Safe.name().to_string(),
            theme_origin: None,
            palette: Palette::Safe,
            overrides: Vec::new(),
            warn: Theme::DEFAULT_WARN_PCT,
            critical: Theme::DEFAULT_CRITICAL_PCT,
            interval: crate::app::DEFAULT_INTERVAL,
            window: DEFAULT_WINDOW,
        }
    }
}

/// Ten minutes, the span the buffer has always held at one sample a second.
pub const DEFAULT_WINDOW: Duration = Duration::from_secs(600);

/// Bounds on the sample rate.
///
/// Below the floor the collector is most of what the machine is doing: a pass
/// costs about 1ms at 400 processes, so 50ms spends 2% of a core and 10ms
/// would spend 10% — a monitor that is itself the load is not measuring the
/// machine, it is measuring itself. Above the ceiling the timeline stops being
/// a timeline.
const MIN_INTERVAL: Duration = Duration::from_millis(50);
const MAX_INTERVAL: Duration = Duration::from_secs(60);
const MIN_WINDOW: Duration = Duration::from_secs(10);

/// The most samples the ring buffer will be asked to hold.
///
/// The limit belongs to the *product* of the two settings, not to either: a
/// day of history at one a second and ten minutes at sixty a second cost the
/// same. Every sample retains its whole process table — that is what makes
/// scrubbing show the real table from that instant — so the buffer is roughly
/// `samples * processes * 96 bytes`, or about 3.3 GB at this cap on a
/// 400-process box. See the README.
///
/// Stated as the intent — a day of history at one sample a second — rather
/// than as a round number, plus the one extra sample a span of that length
/// needs. See [`Settings::history_len`].
const MAX_SAMPLES: usize = 24 * 60 * 60 + 1;

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
    ("warn", |s, v| {
        s.warn = percentage(v)?;
        Ok(())
    }),
    ("critical", |s, v| {
        s.critical = percentage(v)?;
        Ok(())
    }),
    ("interval", |s, v| {
        s.interval = duration(v)?;
        Ok(())
    }),
    ("window", |s, v| {
        s.window = duration(v)?;
        Ok(())
    }),
    ("theme", |s, v| {
        // Any name parses. It may name a file this table cannot see, and
        // rejecting unknown names here would make user themes impossible.
        // `resolve` settles whether it exists.
        s.theme = match v {
            "auto" | "default" => Palette::default().name().to_string(),
            _ => v.to_string(),
        };
        Ok(())
    }),
];

/// A percentage, which is what every threshold in ptop is.
///
/// Rejecting the out-of-range value rather than clamping it: `warn = 150` is
/// someone who has misunderstood the units, and silently turning it into 100
/// hides that from them for as long as they use the tool.
fn percentage(v: &str) -> Result<f32, &'static str> {
    match v.parse::<f32>() {
        Ok(n) if (0.0..=100.0).contains(&n) => Ok(n),
        _ => Err("a percentage from 0 to 100"),
    }
}

/// A span, written the way people write them: `500ms`, `2s`, `10m`, `1h`.
///
/// A bare number is seconds, because that is what someone typing `interval = 2`
/// means, and guessing milliseconds there would silently sample five hundred
/// times too fast.
fn duration(v: &str) -> Result<Duration, &'static str> {
    const EXPECTED: &str = "a span like `500ms`, `2s`, `10m` or `1h`";
    let (digits, scale) = match v {
        _ if v.ends_with("ms") => (&v[..v.len() - 2], 0.001),
        _ if v.ends_with('s') => (&v[..v.len() - 1], 1.0),
        _ if v.ends_with('m') => (&v[..v.len() - 1], 60.0),
        _ if v.ends_with('h') => (&v[..v.len() - 1], 3600.0),
        _ => (v, 1.0),
    };
    // Bounded before converting, not after: `Duration::from_secs_f64` panics
    // outside its range, so `window = 99999999999999999999` would have aborted
    // the process — from a *config file*, which is the one place this module
    // promises a typo cannot cost you the tool. A year is far past anything
    // either setting will accept; `check_buffer` applies the real limits.
    const MAX_SECS: f64 = 366.0 * 24.0 * 3600.0;
    match digits.trim().parse::<f64>() {
        Ok(n) if n.is_finite() && n > 0.0 && n * scale <= MAX_SECS => {
            Ok(Duration::from_secs_f64(n * scale))
        }
        _ => Err(EXPECTED),
    }
}

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
    /// Two settings that are each fine and wrong together.
    Pair(String),
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
            Self::Pair(why) => write!(f, "{why}"),
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
            Self::UnknownKey { .. } | Self::Pair(_) => self.to_string(),
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
#[derive(Debug)]
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
    sources: Sources<'_>,
    args: &[String],
) -> Result<(Settings, Vec<String>, Vec<Warning>), Bad> {
    let Sources {
        file,
        no_color,
        themes,
    } = sources;
    let mut warnings = Vec::new();
    if let Some((origin, text)) = file {
        apply_file(&mut settings, text, origin, &mut warnings);
    }
    // NO_COLOR outranks the file. The file records a preference in general;
    // the environment is the user saying something about this terminal now.
    if no_color {
        settings.tier = Tier::Mono;
    }

    // A check no single key can make on its own, so it cannot live in the
    // table. Run twice, once per source, because the disposition differs: a
    // file that leaves the pair meaningless falls back to the defaults with a
    // warning, the way every other bad line does, while a flag that does it is
    // fatal, the way every other bad flag is.
    if let Err(why) = check_settings(&settings) {
        // Reverting every cross-key setting, not just the pair that failed:
        // they are checked together, so a half-reverted state has not been
        // checked at all. Named in the warning because the revert can undo a
        // value the file never set — a file saying only `critical = 40` is put
        // back to 50/80, and this is the user's only sign of it.
        let d = Settings {
            warn: Theme::DEFAULT_WARN_PCT,
            critical: Theme::DEFAULT_CRITICAL_PCT,
            interval: crate::app::DEFAULT_INTERVAL,
            window: DEFAULT_WINDOW,
            ..settings
        };
        warnings.push(Warning(format!(
            "{}: {why} — using the defaults: warn {}, critical {}, interval {:?}, window {:?}",
            origin_of(&file),
            d.warn,
            d.critical,
            d.interval,
            d.window,
        )));
        settings = d;
    }

    let mut positional = Vec::new();
    for a in args {
        // Flags are the same settings the file takes, so one table serves both
        // and neither can drift. A `--flag=` naming no setting falls through
        // to be reported as an unrecognised option, which is what it is.
        match a.strip_prefix("--").and_then(|f| f.split_once('=')) {
            Some((key, value)) if KEYS.iter().any(|(k, _)| *k == key) => {
                apply(&mut settings, key, value)?;
                if key == "theme" {
                    settings.theme_origin = None;
                }
            }
            _ => positional.push(a.clone()),
        }
    }
    check_settings(&settings).map_err(Bad::Pair)?;

    // Themes resolve last, because the name is a setting like any other and
    // the last source to set it wins.
    //
    // Which source that was decides what a failure costs, the same asymmetry
    // as everywhere else: a name typed for this run is fatal, a name sitting
    // in a file falls back to the default with a warning. Checked by looking
    // for the flag rather than by threading an origin through the table,
    // because the flag is the thing that makes it fatal.
    let from_flag = args.iter().any(|a| a.starts_with("--theme="));
    match resolve_named_theme(&settings.theme, themes, &mut warnings) {
        Ok((palette, overrides)) => {
            settings.palette = palette;
            settings.overrides = overrides;
        }
        Err(why) if from_flag => return Err(Bad::Pair(why)),
        Err(why) => {
            // Anchored to the line that named it, like every other config
            // warning. A theme is resolved after the file is fully parsed, so
            // without the remembered origin this is the one error that could
            // not point at anything.
            let at = settings
                .theme_origin
                .as_ref()
                .map_or(String::new(), |o| format!("{o}: "));
            warnings.push(Warning(format!(
                "{at}{why} — using `{}`",
                Palette::default().name()
            )));
            settings.theme = Palette::default().name().to_string();
            settings.palette = Palette::default();
        }
    }

    Ok((settings, positional, warnings))
}

/// What to say about a theme this terminal could not fully use, if anything.
///
/// Here rather than in `main` so the wording is testable. It is the only part
/// of theme handling a user sees when it goes right-ish, and there are two
/// quite different things to say.
pub fn theme_note(
    tier: Tier,
    name: &str,
    theme: &Theme,
    overrides: &[(Token, Color)],
    skipped: &[Token],
) -> Option<String> {
    if overrides.is_empty() {
        return None;
    }
    // Monochrome does not *fail* to show a theme, it declines to — a different
    // thing to be told. Listing ten tokens it "cannot show" and ten
    // replacements all reading `default` names no line the user could rewrite,
    // and it would fire on every run under NO_COLOR, TERM=dumb, or no TERM at
    // all, which is most of CI.
    if tier == Tier::Mono {
        return Some(format!(
            "theme `{name}` is not used: this terminal is monochrome, where meaning \
             never rests on colour anyway"
        ));
    }
    if skipped.is_empty() {
        return None;
    }
    // Named, not counted. "3 colours were dropped" tells a user they have a
    // problem; naming them and what was kept instead tells them which lines to
    // rewrite, which is the only thing they can act on.
    Some(format!(
        "theme `{name}`: this terminal is {tier:?} and cannot show {}; keeping {}",
        skipped
            .iter()
            .map(|t| t.name())
            .collect::<Vec<_>>()
            .join(", "),
        skipped
            .iter()
            .map(|&tok| format!(
                "{} = {}",
                tok.name(),
                crate::theme::write_color(tok.get(theme))
            ))
            .collect::<Vec<_>>()
            .join(", "),
    ))
}

/// Reads a named theme, returning its origin and contents or why not.
///
/// Injected rather than called directly so the theme rules can be tested
/// without a filesystem, the same way `Sources::file` is.
pub type ThemeReader<'a> = &'a dyn Fn(&str) -> Result<(String, String), String>;

/// Where settings come from, other than the command line.
///
/// Taken rather than read, so precedence and the theme rules can be tested
/// without a filesystem or a process-global environment.
pub struct Sources<'a> {
    pub file: Option<(&'a str, &'a str)>,
    pub no_color: bool,
    /// A named theme's origin and contents, or why it could not be read.
    pub themes: ThemeReader<'a>,
}

/// A theme name to a base palette and the colours written over it.
///
/// Built-ins win. A user file called `safe.theme` would otherwise shadow the
/// palette that everything else in this project is measured against, and
/// silently — the shadowing would be invisible in every message ptop prints.
pub fn resolve_named_theme(
    name: &str,
    themes: ThemeReader<'_>,
    warnings: &mut Vec<Warning>,
) -> Result<(Palette, Vec<(Token, Color)>), String> {
    if let Some(builtin) = Palette::parse(name) {
        return Ok((builtin, Vec::new()));
    }
    let (origin, text) = themes(name).map_err(|why| {
        match closest_in(name, &[Palette::Safe.name(), Palette::Classic.name()]) {
            Some(k) => format!("{why} (did you mean `{k}`?)"),
            None => why,
        }
    })?;
    // Inheriting from `safe` rather than from nothing, so a theme that sets
    // two colours is two lines. Requiring all ten is why people do not write
    // themes.
    Ok((Palette::Safe, parse_theme(&text, &origin, warnings)))
}

/// The thresholds have to be usable as a pair, which neither key can tell
/// alone.
///
/// Stated as one rule: every status band must be reachable. `heat` reads
/// `[0, warn)` as ok, `[warn, critical)` as warn and `[critical, 100]` as
/// critical, so `warn` above zero and `critical` above `warn` is exactly the
/// condition for none of the three to be empty.
///
/// That covers `warn == critical`, which would leave the warn band empty, and
/// `warn = 0`, which leaves nothing ever `ok` and draws a permanent rule along
/// the bottom of both graphs. `critical = 100` is allowed: its band is the
/// single point 100, but a machine really does reach 100% memory, so the band
/// is reachable rather than empty.
fn check_settings(s: &Settings) -> Result<(), String> {
    check_thresholds(s)?;
    check_buffer(s)
}

/// Bounds on the sample rate and on what the two rate settings cost together.
///
/// The sample count is checked rather than the window, because the window on
/// its own says nothing: a day of history at one sample a second and ten
/// minutes at sixty a second are the same buffer.
fn check_buffer(s: &Settings) -> Result<(), String> {
    if s.interval < MIN_INTERVAL || s.interval > MAX_INTERVAL {
        return Err(format!(
            "`interval` is {:?}; it must be between {MIN_INTERVAL:?} and {MAX_INTERVAL:?}",
            s.interval
        ));
    }
    if s.window < MIN_WINDOW {
        return Err(format!(
            "`window` is {:?}; it must be at least {MIN_WINDOW:?}",
            s.window
        ));
    }
    if s.window < s.interval {
        return Err(format!(
            "`window` is {:?} but `interval` is {:?}; the window must hold at least one sample",
            s.window, s.interval
        ));
    }
    let samples = s.history_len();
    if samples > MAX_SAMPLES {
        return Err(format!(
            "`window` {:?} at `interval` {:?} is {samples} samples, above the limit of {MAX_SAMPLES}; \
             every sample retains a whole process table, which is about {} MB \
             on a 400-process box",
            s.window,
            s.interval,
            estimated_mb(samples, 400),
        ));
    }
    Ok(())
}

/// Roughly what a buffer of this shape costs, in megabytes.
///
/// Rough on purpose: the figure exists to make "that is a lot of memory"
/// concrete, and a reader who needs it to the byte is asking the wrong
/// question of a monitor. The `show_sample_footprint` test prints the real
/// sizes this is derived from.
fn estimated_mb(samples: usize, procs: usize) -> usize {
    let per = size_of::<Sample>() + procs * size_of::<ProcSample>();
    per.saturating_mul(samples) / 1_000_000
}

fn check_thresholds(s: &Settings) -> Result<(), String> {
    if s.warn <= 0.0 {
        return Err(format!(
            "`warn` is {}, so nothing is ever ok; it must be above 0",
            s.warn
        ));
    }
    if s.warn >= s.critical {
        return Err(format!(
            "`warn` is {} and `critical` is {}; warn must be below critical",
            s.warn, s.critical
        ));
    }
    Ok(())
}

/// What to blame a cross-key problem on when the file is where it came from.
fn origin_of(file: &Option<(&str, &str)>) -> String {
    file.map_or_else(|| "ptop".to_string(), |(origin, _)| origin.to_string())
}

/// Read the user's config file, if there is one.
///
/// A missing file is not an error — the overwhelmingly common case is not
/// having one — and neither is an unreadable one, which is worth a word rather
/// than a refusal to start.
/// Read a named theme from `~/.config/ptop/themes/NAME.theme`.
///
/// Beside the config file rather than inside it: a theme is a document people
/// swap, paste and publish, and a format you can send someone as one file is
/// the whole reason btop has 41 themes and htop has eight.
pub fn read_theme(name: &str) -> Result<(String, String), String> {
    // A theme name becomes a path, so it must not be able to leave the themes
    // directory. `--theme=../../../etc/passwd` is a mistake worth refusing
    // rather than a traversal worth worrying about — nothing here runs with
    // privilege — but reading an arbitrary file and reporting its contents as
    // bad colour values is a poor way to answer it either way.
    if name.is_empty() || name.contains(['/', '\\']) || name.contains("..") {
        return Err(format!("`{name}` is not a theme name"));
    }
    let Some(dir) = path().and_then(|p| p.parent().map(|d| d.join("themes"))) else {
        return Err(format!(
            "no theme `{name}`, and no home directory to look in"
        ));
    };
    let path = dir.join(format!("{name}.theme"));
    match std::fs::read_to_string(&path) {
        Ok(text) => Ok((path.display().to_string(), text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(format!(
            "no theme `{name}`: not a built-in, and {} does not exist",
            path.display()
        )),
        Err(e) => Err(format!("theme `{name}`: {}: {e}", path.display())),
    }
}

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
    for_each_setting(text, origin, warnings, |key, value, line| {
        apply(settings, key, value).map_err(|bad| bad.to_string())?;
        if key == "theme" {
            settings.theme_origin = Some(format!("{origin}:{line}"));
        }
        Ok(())
    });
}

/// Walk a `key = value` file, handing each usable line to `f` and reporting
/// the rest.
///
/// Shared by the config file and by theme files. They hold different keys but
/// the same grammar, and two copies of a grammar drift — the leading-`#`
/// exemption exists *for* theme files, so a second parser would be the one
/// that got it wrong.
fn for_each_setting(
    text: &str,
    origin: &str,
    warnings: &mut Vec<Warning>,
    mut f: impl FnMut(&str, &str, usize) -> Result<(), String>,
) {
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
        if let Err(why) = f(key, value, line) {
            warn(why);
        }
    }
}

/// Read a theme file into token overrides.
///
/// Every token is optional. Requiring all ten is the reason people do not
/// write themes — btop ships 41 because a theme is one file and one line per
/// colour, htop has eight hardcoded in C and nobody has ever added a ninth.
/// Overriding two colours should be two lines.
///
/// A malformed value warns and that token alone falls back, rather than
/// failing the file: the same rule as the config file, for the same reason.
pub fn parse_theme(text: &str, origin: &str, warnings: &mut Vec<Warning>) -> Vec<(Token, Color)> {
    let mut out = Vec::new();
    for_each_setting(text, origin, warnings, |key, value, _| {
        let token = Token::parse(key).ok_or_else(|| {
            let hint = closest_in(key, &Token::ALL.map(Token::name))
                .map_or(String::new(), |k| format!(" (did you mean `{k}`?)"));
            format!("unknown colour `{key}`{hint}")
        })?;
        let colour = crate::theme::parse_color(value).ok_or_else(|| {
            format!("`{key}`: expected a colour like `#5ccfe6`, `80` or `cyan`, found `{value}`")
        })?;
        out.push((token, colour));
        Ok(())
    });
    out
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
    closest_in(key, &KEYS.iter().map(|&(k, _)| k).collect::<Vec<_>>())
}

/// The nearest name in a set, or none if nothing is near enough.
///
/// Two edits, the conventional threshold, rather than a bound that scales with
/// the candidate's length. Scaling looked reasonable and was far too loose on
/// short names: `nope` is three edits from `safe` on a four-letter word, and
/// ptop offered it. A confidently wrong hint is worse than none, and it is
/// worst when the tool sounds sure.
fn closest_in(key: &str, names: &[&'static str]) -> Option<&'static str> {
    names
        .iter()
        .map(|&k| (distance(key, k), k))
        .filter(|&(d, _)| d <= 2)
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
        let mut s = Settings::fixed();
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
                // `theme`, not `palette`: the name is the setting, and the
                // palette is what `resolve` turns it into once it can see
                // whether a file of that name exists.
                theme: "classic".to_string(),
                // …and the line it came from, kept because a theme is the one
                // setting judged somewhere other than where it is read.
                theme_origin: Some("conf:3".to_string()),
                ..Settings::fixed()
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
             color = ultraviolet\n\
             color =\n\
             theme = classic\n",
        );
        assert_eq!(
            s.glyphs,
            GlyphSet::Block,
            "a line before the bad ones was lost"
        );
        assert_eq!(s.theme, "classic", "a line after them was lost");
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
        // Three edits on a four-letter word is not a near miss. ptop used to
        // answer `theme = nope` with "did you mean `safe`?" — the name it was
        // about to fall back to anyway.
        assert_eq!(closest_in("nope", &["safe", "classic"]), None);
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
        let mut s = Settings::fixed();
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
        Settings::fixed()
    }

    fn run(file: Option<&str>, no_color: bool, args: &[&str]) -> (Settings, Vec<String>) {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let (s, positional, _) = resolve(
            base(),
            Sources {
                file: file.map(|t| ("conf", t)),
                no_color,
                themes: &no_themes,
            },
            &args,
        )
        .expect("flags are valid");
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
        assert!(
            resolve(
                base(),
                Sources {
                    file: None,
                    no_color: false,
                    themes: &no_themes
                },
                &args
            )
            .is_err()
        );
    }

    #[test]
    fn a_flag_naming_no_setting_is_left_for_the_caller_to_reject() {
        // `--wat=1` is an unrecognised option, not a config error, and must
        // reach the usage message rather than being swallowed here.
        let (_, positional) = run(None, false, &["--wat=1", "--once"]);
        assert_eq!(positional, vec!["--wat=1", "--once"]);
    }
}

#[cfg(test)]
mod thresholds {
    use super::*;

    fn base() -> Settings {
        Settings::fixed()
    }

    fn run(file: Option<&str>, args: &[&str]) -> Result<(Settings, Vec<Warning>), Bad> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        resolve(
            base(),
            Sources {
                file: file.map(|t| ("conf", t)),
                no_color: false,
                themes: &no_themes,
            },
            &args,
        )
        .map(|(s, _, w)| (s, w))
    }

    #[test]
    fn both_thresholds_are_settable() {
        let (s, w) = run(Some("warn = 65\ncritical = 90\n"), &[]).unwrap();
        assert!(
            w.is_empty(),
            "{:?}",
            w.iter().map(|x| &x.0).collect::<Vec<_>>()
        );
        assert_eq!((s.warn, s.critical), (65.0, 90.0));

        let (s, _) = run(None, &["--warn=20", "--critical=40"]).unwrap();
        assert_eq!((s.warn, s.critical), (20.0, 40.0));
    }

    #[test]
    fn a_threshold_outside_a_percentage_is_rejected() {
        for bad in ["150", "-1", "", "half"] {
            let (s, w) = run(Some(&format!("warn = {bad}\n")), &[]).unwrap();
            assert_eq!(
                s.warn,
                Theme::DEFAULT_WARN_PCT,
                "`warn = {bad}` was accepted"
            );
            assert_eq!(w.len(), 1, "`warn = {bad}` went unreported");
        }
        // 100 is the end of the range, not outside it.
        assert_eq!(
            run(Some("critical = 100\n"), &[]).unwrap().0.critical,
            100.0
        );
        // …and a fraction is a percentage. The float type is the whole reason
        // the header prints `{}` rather than `{:.0}`.
        assert_eq!(run(Some("warn = 62.5\n"), &[]).unwrap().0.warn, 62.5);
    }

    #[test]
    fn a_pair_that_makes_a_status_unreachable_is_rejected() {
        // One rule — every status band must be reachable — so all three ways
        // of breaking it are checked against it. `heat` reads [0, warn) as ok,
        // [warn, critical) as warn and [critical, 100] as critical.
        for (warn, critical) in [
            (90.0, 80.0), // inverted: the warn band runs backwards
            (80.0, 80.0), // equal: the warn band is empty
            (0.0, 80.0),  // nothing is ever ok
        ] {
            let (s, w) = run(
                Some(&format!("warn = {warn}\ncritical = {critical}\n")),
                &[],
            )
            .unwrap();
            assert_eq!(
                (s.warn, s.critical),
                (Theme::DEFAULT_WARN_PCT, Theme::DEFAULT_CRITICAL_PCT),
                "{warn}/{critical} was left in place"
            );
            let said = |needle: &str| w.iter().any(|x| x.0.contains(needle));
            assert!(
                said(&format!("{warn}")),
                "{warn}/{critical}: the warning does not name the offending value: {:?}",
                w.iter().map(|x| &x.0).collect::<Vec<_>>()
            );
            // The recovery reverts *both*, including one the file may never
            // have set, so the warning has to say what it fell back to.
            assert!(
                said("default") && said("50") && said("80"),
                "{warn}/{critical}: the warning does not say what it fell back to: {:?}",
                w.iter().map(|x| &x.0).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn a_bad_pair_from_flags_is_fatal_where_the_same_pair_in_a_file_is_not() {
        // Same asymmetry as every other setting: the file is read every run,
        // the flags were typed for this one.
        assert!(run(Some("warn = 90\n"), &[]).is_ok());
        assert!(run(None, &["--warn=90"]).is_err());
    }

    #[test]
    fn a_flag_can_rescue_a_pair_the_file_got_wrong() {
        // The file's pair is reverted, then the flag applies to the defaults —
        // so `--critical=95` alongside a bad file still leaves a usable pair
        // rather than compounding into a second failure.
        let (s, w) = run(Some("warn = 90\n"), &["--critical=95"]).unwrap();
        assert_eq!((s.warn, s.critical), (Theme::DEFAULT_WARN_PCT, 95.0));
        assert_eq!(w.len(), 1);
    }
}

#[cfg(test)]
mod rate {
    use super::*;

    fn run(file: &str, args: &[&str]) -> Result<(Settings, Vec<Warning>), Bad> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        resolve(
            Settings::fixed(),
            Sources {
                file: Some(("conf", file)),
                no_color: false,
                themes: &no_themes,
            },
            &args,
        )
        .map(|(s, _, w)| (s, w))
    }

    #[test]
    fn a_span_is_written_the_way_people_write_spans() {
        for (text, secs) in [
            ("500ms", 0.5),
            ("2s", 2.0),
            ("10m", 600.0),
            ("1h", 3600.0),
            // A bare number is seconds: it is what someone typing
            // `interval = 2` means, and guessing milliseconds there would
            // silently sample five hundred times too fast.
            ("2", 2.0),
            ("1.5s", 1.5),
        ] {
            assert_eq!(
                duration(text).map(|d| d.as_secs_f64()),
                Ok(secs),
                "`{text}` did not parse as {secs}s"
            );
        }
        for text in [
            "",
            "soon",
            "0s",
            "-1s",
            "1x",
            "s",
            "inf",
            "NaN",
            // Past what a Duration can hold. `from_secs_f64` *panics* there,
            // so this used to abort the process — from a config file, which
            // is the one place a typo is promised not to cost you the tool.
            "99999999999999999999",
            "1e300",
            "9999999h",
        ] {
            assert!(duration(text).is_err(), "`{text}` was accepted");
        }
    }

    #[test]
    fn the_window_is_a_span_and_the_buffer_follows_it() {
        // Sample count is a fact about the buffer; the span is what the user
        // wants. Halving the interval over the same window doubles the buffer.
        let (s, w) = run("window = 10m\ninterval = 1s\n", &[]).unwrap();
        assert!(
            w.is_empty(),
            "{:?}",
            w.iter().map(|x| &x.0).collect::<Vec<_>>()
        );
        assert_eq!(s.history_len(), 601);

        let (s, _) = run("window = 10m\ninterval = 500ms\n", &[]).unwrap();
        assert_eq!(s.history_len(), 1201);

        // The buffer must hold *at least* the window. n samples span n - 1
        // intervals, so an off-by-one here silently shortens everyone's
        // history by one tick: 4 samples at 3s cover 9s, not the 10s asked for.
        for (window, interval, want) in [(10.0, 3.0, 5usize), (600.0, 1.0, 601), (10.0, 1.0, 11)] {
            let (s, _) = run(
                &format!("window = {window}s\ninterval = {interval}s\n"),
                &[],
            )
            .unwrap();
            assert_eq!(s.history_len(), want, "{window}s at {interval}s");
            let covered = (want - 1) as f64 * interval;
            assert!(
                covered >= window,
                "{want} samples at {interval}s cover {covered}s, short of the {window}s asked for"
            );
        }
    }

    #[test]
    fn a_rate_the_collector_cannot_sustain_is_rejected() {
        // A pass costs about 1ms at 400 processes, so 50ms already spends 2%
        // of a core. A monitor that is itself the load is measuring itself.
        for bad in ["1ms", "10ms", "5m"] {
            let (s, w) = run(&format!("interval = {bad}\n"), &[]).unwrap();
            assert_eq!(s.interval, crate::app::DEFAULT_INTERVAL, "`{bad}` stuck");
            assert!(!w.is_empty(), "`{bad}` went unreported");
        }
        assert!(run("interval = 50ms\n", &[]).unwrap().1.is_empty());
        assert!(run("interval = 60s\n", &[]).unwrap().1.is_empty());
    }

    #[test]
    fn the_limit_belongs_to_the_pair_not_to_either_setting() {
        // A day at one a second and ten minutes at sixty a second are the same
        // buffer, so neither number alone can be judged.
        // A day at one a second is exactly the cap, and must be accepted:
        // a limit that rejects the case it was sized for is off by one.
        assert!(
            run("window = 24h\ninterval = 1s\n", &[])
                .unwrap()
                .1
                .is_empty()
        );
        let (_, w) = run("window = 24h\ninterval = 500ms\n", &[]).unwrap();
        assert_eq!(w.len(), 1, "twice the cap was accepted");
        // And the complaint says what it would cost, since "too many samples"
        // is not a quantity anyone can feel.
        assert!(w[0].0.contains("MB"), "{}", w[0].0);
    }

    #[test]
    fn a_window_shorter_than_one_sample_is_rejected() {
        let (_, w) = run("window = 10s\ninterval = 30s\n", &[]).unwrap();
        assert_eq!(
            w.len(),
            1,
            "a buffer that cannot hold a sample was accepted"
        );
    }
}

#[cfg(test)]
mod themes {
    use super::*;

    /// A theme reader over a fixed set, so the rules are testable without a
    /// filesystem.
    fn from(
        pairs: &'static [(&'static str, &'static str)],
    ) -> impl Fn(&str) -> Result<(String, String), String> {
        move |name| {
            pairs
                .iter()
                .find(|(n, _)| *n == name)
                .map(|(n, text)| (format!("{n}.theme"), text.to_string()))
                .ok_or_else(|| format!("no theme `{name}`"))
        }
    }

    fn run(
        args: &[&str],
        file: Option<&str>,
        themes: ThemeReader<'_>,
    ) -> Result<(Settings, Vec<Warning>), Bad> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        resolve(
            Settings::fixed(),
            Sources {
                file: file.map(|t| ("conf", t)),
                no_color: false,
                themes,
            },
            &args,
        )
        .map(|(s, _, w)| (s, w))
    }

    const NORD: &str = "ok = #8fbcbb\nseries_cpu = 67\n";

    #[test]
    fn a_theme_may_set_any_subset_and_inherit_the_rest() {
        // Requiring all ten is the reason people do not write themes.
        let (s, w) = run(&["--theme=nord"], None, &from(&[("nord", NORD)])).unwrap();
        assert!(
            w.is_empty(),
            "{:?}",
            w.iter().map(|x| &x.0).collect::<Vec<_>>()
        );
        assert_eq!(s.palette, Palette::Safe, "a partial theme lost its base");
        assert_eq!(
            s.overrides,
            vec![
                (Token::Ok, Color::Rgb(0x8f, 0xbc, 0xbb)),
                (Token::SeriesCpu, Color::Indexed(67)),
            ]
        );
    }

    #[test]
    fn a_builtin_cannot_be_shadowed_by_a_file() {
        // A user file called `safe.theme` would otherwise silently replace the
        // palette everything in this project is measured against.
        let shadow = from(&[("safe", "ok = #ff0000\n")]);
        let (s, _) = run(&["--theme=safe"], None, &shadow).unwrap();
        assert_eq!(s.palette, Palette::Safe);
        assert!(s.overrides.is_empty(), "a file shadowed the built-in");
    }

    #[test]
    fn one_bad_colour_falls_back_alone() {
        // Good lines on *both* sides of the bad ones. With the good line last,
        // a theme that threw away everything it had on each new colour would
        // still pass — it kept the final one either way.
        let (s, w) = run(
            &["--theme=t"],
            None,
            &from(&[(
                "t",
                "ok = #8fbcbb\nwarn = notacolour\ncritical\nnosuch = red\nlive = 80\n",
            )]),
        )
        .unwrap();
        assert_eq!(
            s.overrides,
            vec![
                (Token::Ok, Color::Rgb(0x8f, 0xbc, 0xbb)),
                (Token::Live, Color::Indexed(80)),
            ],
            "a good colour was lost to a bad one on another line"
        );
        assert_eq!(
            w.len(),
            3,
            "{:?}",
            w.iter().map(|x| &x.0).collect::<Vec<_>>()
        );
        assert!(
            w[0].0.contains("t.theme:2") && w[0].0.contains("notacolour"),
            "{}",
            w[0].0
        );
        assert!(w[1].0.contains("t.theme:3"), "{}", w[1].0);
        // No suggestion for `nosuch`: it is six edits from every token, and a
        // confidently wrong hint is worse than none.
        assert!(w[2].0.contains("nosuch"), "{}", w[2].0);
        assert!(!w[2].0.contains("did you mean"), "{}", w[2].0);
    }

    #[test]
    fn a_missing_theme_is_fatal_from_a_flag_and_a_warning_from_the_file() {
        // The same asymmetry as every other setting: a name typed for this run
        // is fatal; a name sitting in a file falls back and says so.
        assert!(run(&["--theme=nope"], None, &from(&[])).is_err());

        let (s, w) = run(&[], Some("theme = nope\n"), &from(&[])).unwrap();
        assert_eq!(
            s.palette,
            Palette::Safe,
            "a missing theme was not fallen back from"
        );
        assert_eq!(w.len(), 1);
        assert!(
            w[0].0.contains("nope") && w[0].0.contains("safe"),
            "{}",
            w[0].0
        );
    }

    #[test]
    fn a_bad_theme_in_the_file_points_at_its_line() {
        // Every other config warning reads `conf:4: ...`. A theme is resolved
        // after the whole file is parsed, so without carrying the origin
        // forward this would be the one error that cannot point at anything.
        let (_, w) = run(
            &[],
            Some("glyphs = block\ncolor = 256\ntheme = nope\n"),
            &from(&[]),
        )
        .unwrap();
        assert_eq!(w.len(), 1);
        assert!(w[0].0.starts_with("conf:3: "), "{}", w[0].0);
    }

    #[test]
    fn a_flag_theme_reports_no_file_line() {
        // The flag is where it came from, so a stale line from the file would
        // point at something that is not the problem.
        let (s, _) = run(&["--theme=safe"], Some("theme = classic\n"), &from(&[])).unwrap();
        assert_eq!(s.theme_origin, None);
    }

    #[test]
    fn a_misspelt_builtin_is_guessed() {
        // The commonest `--theme` typo. It used to be caught by the key table
        // and lost its suggestion when theme names stopped validating there.
        let err = run(&["--theme=clasic"], None, &from(&[])).unwrap_err();
        assert!(err.to_string().contains("did you mean `classic`?"), "{err}");
    }

    #[test]
    fn monochrome_says_one_thing_rather_than_ten() {
        use crate::theme::Tier;
        let overrides = Token::ALL.map(|t| (t, Color::Red)).to_vec();
        let (theme, skipped) = Theme::new(Palette::Safe, Tier::Mono).with_overrides(&overrides);

        let note = theme_note(Tier::Mono, "nord", &theme, &overrides, &skipped)
            .expect("a mono user with a theme should hear something");
        assert!(note.contains("monochrome"), "{note}");
        assert!(
            !note.contains("series_cpu"),
            "mono listed every token it 'cannot show': {note}"
        );

        // No theme, nothing to say — this fires on every run under NO_COLOR.
        assert_eq!(theme_note(Tier::Mono, "safe", &theme, &[], &[]), None);
    }

    #[test]
    fn an_ansi_index_works_on_an_ansi_terminal() {
        use crate::theme::Tier;
        // 0-15 *are* the ANSI slots: `ok = 6` and `ok = cyan` name the same
        // thing, and a 16-colour terminal renders both.
        let overrides = vec![
            (Token::Ok, Color::Indexed(6)),
            (Token::Warn, Color::Indexed(222)),
        ];
        let (theme, skipped) = Theme::new(Palette::Safe, Tier::Ansi16).with_overrides(&overrides);
        assert_eq!(skipped, vec![Token::Warn], "an ANSI slot was refused");
        assert_eq!(Token::get(Token::Ok, &theme), Color::Indexed(6));
    }

    #[test]
    fn a_theme_name_cannot_walk_out_of_the_themes_directory() {
        for name in ["../../../etc/passwd", "a/b", "..", ""] {
            assert!(read_theme(name).is_err(), "`{name}` was accepted as a name");
        }
    }

    #[test]
    fn the_shipped_themes_match_the_compiled_ones() {
        // A theme file is how people learn the format, so the two shipped ones
        // have to actually be the palettes they claim. Regenerate with the
        // `write_builtin_theme_files` test after changing a palette.
        for (palette, text) in [
            (Palette::Safe, include_str!("../themes/safe.theme")),
            (Palette::Classic, include_str!("../themes/classic.theme")),
        ] {
            let built_in = Theme::new(palette, Tier::TrueColor);
            let mut w = Vec::new();
            let overrides = parse_theme(text, "shipped", &mut w);
            assert!(
                w.is_empty(),
                "{:?}",
                w.iter().map(|x| &x.0).collect::<Vec<_>>()
            );
            assert_eq!(
                overrides.len(),
                Token::ALL.len(),
                "{}.theme does not name every colour",
                palette.name()
            );
            for (token, colour) in overrides {
                assert_eq!(
                    colour,
                    token.get(&built_in),
                    "{}.theme has the wrong `{}` — regenerate it",
                    palette.name(),
                    token.name()
                );
            }
        }
    }
}
