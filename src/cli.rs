//! Command-line parsing and the terminal colour palette.
//!
//! The parse is hand-rolled rather than clap-based to keep the dependency
//! footprint small. Everything that decides what to print returns a value so it
//! can be tested without a terminal; only the thin [`options`] wrapper exits.

use std::env;
use std::fmt;
use std::io::IsTerminal;

/// A single ANSI style: an optional 24-bit colour and a bold flag. `paint` is a
/// no-op when colour is disabled, so callers can build a line unconditionally
/// and let the terminal decide.
#[derive(Clone, Copy)]
struct Style {
    rgb: Option<(u8, u8, u8)>,
    bold: bool,
}

impl Style {
    const fn fg(r: u8, g: u8, b: u8) -> Self {
        Self {
            rgb: Some((r, g, b)),
            bold: false,
        }
    }

    fn paint(self, text: &str, color: bool) -> String {
        if !color {
            return text.to_owned();
        }
        let mut out = String::new();
        if self.bold {
            out.push_str("\x1b[1m");
        }
        if let Some((r, g, b)) = self.rgb {
            out.push_str(&format!("\x1b[38;2;{r};{g};{b}m"));
        }
        out.push_str(text);
        out.push_str("\x1b[0m");
        out
    }
}

// Palette drawn from the overlay and site: cool blue headings, a violet accent
// for flags and the version, and the same amber used for the active window.
const HEADING: Style = Style {
    rgb: Some((0xd9, 0xe2, 0xff)),
    bold: true,
};
const ACCENT: Style = Style {
    rgb: Some((0x8f, 0xa8, 0xff)),
    bold: true,
};
const FLAG: Style = Style::fg(0xb1, 0x8c, 0xff);
const MUTED: Style = Style::fg(0x8a, 0x8a, 0x9a);
const ERROR: Style = Style {
    rgb: Some((0xff, 0x7a, 0x9c)),
    bold: true,
};
const WARNING: Style = Style {
    rgb: Some((0xff, 0xca, 0x85)),
    bold: true,
};
const VERSION: Style = Style {
    rgb: Some((0xb1, 0x8c, 0xff)),
    bold: true,
};

const NAME: &str = "hyprview";
const USAGE: &str = "hyprview [OPTIONS]";
const ABOUT: &str = "a fast, keyboard-driven workspace overview";

/// Whether to emit ANSI colour for a stream. Colour is for a human at a
/// terminal only: pipes and log files get plain text, and `NO_COLOR` opts out
/// even on a TTY.
fn color_enabled(is_terminal: bool) -> bool {
    is_terminal && !no_color()
}

fn no_color() -> bool {
    env::var_os("NO_COLOR").is_some_and(|value| !value.is_empty())
}

fn stdout_color() -> bool {
    color_enabled(std::io::stdout().is_terminal())
}

fn stderr_color() -> bool {
    color_enabled(std::io::stderr().is_terminal())
}

/// `hyprview` shaded from the logo's blue to the site's violet.
#[allow(
    clippy::cast_precision_loss,      // the name is short — no real precision lost
    clippy::cast_possible_truncation, // colour values are bounded 0-255 by construction
    clippy::cast_sign_loss            // colour values are non-negative by construction
)]
fn wordmark(color: bool) -> String {
    const FROM: (f32, f32, f32) = (0x7a as f32, 0xa7 as f32, 0xff as f32);
    const TO: (f32, f32, f32) = (0xb1 as f32, 0x8c as f32, 0xff as f32);
    if !color {
        return NAME.to_owned();
    }
    let steps = NAME.chars().count().saturating_sub(1).max(1) as f32;
    let mut out = String::from("\x1b[1m");
    for (index, ch) in NAME.chars().enumerate() {
        let frac = index as f32 / steps;
        let r = (FROM.0 + (TO.0 - FROM.0) * frac) as u8;
        let g = (FROM.1 + (TO.1 - FROM.1) * frac) as u8;
        let b = (FROM.2 + (TO.2 - FROM.2) * frac) as u8;
        out.push_str(&format!("\x1b[38;2;{r};{g};{b}m{ch}"));
    }
    out.push_str("\x1b[0m");
    out
}

/// The `--version` line. The number comes from `Cargo.toml` at compile time so
/// it cannot drift from the released binary; the optional commit id is absent
/// on the exact release tag and present on local/in-between builds.
fn version_line_with(sha: Option<&str>, color: bool) -> String {
    let mut line = format!(
        "{} {}",
        wordmark(color),
        VERSION.paint(env!("CARGO_PKG_VERSION"), color)
    );
    if let Some(sha) = sha.filter(|sha| !sha.is_empty()) {
        line.push(' ');
        line.push_str(&MUTED.paint(&format!("({sha})"), color));
    }
    line
}

fn version_line(color: bool) -> String {
    version_line_with(option_env!("HYPRVIEW_GIT_SHA"), color)
}

fn print_version() {
    println!("{}", version_line(stdout_color()));
}

fn print_help() {
    let color = stdout_color();
    println!("{} — {ABOUT}", wordmark(color));
    println!();
    println!(
        "{} {}",
        HEADING.paint("Usage:", color),
        ACCENT.paint(USAGE, color)
    );
    println!();
    println!("{}", HEADING.paint("Options:", color));
    option_row(
        color,
        "--toggle",
        "Send SIGUSR1 to the running daemon and exit",
    );
    option_row(
        color,
        "--no-preview",
        "Skip thumbnails and draw coloured placeholder rectangles",
    );
    option_row(
        color,
        "--allow-mouse",
        "Enable mouse hover, click, and right-click support",
    );
    option_row(color, "-h, --help", "Print this help");
    option_row(color, "-V, --version", "Print version");
}

/// One `Options:` row. The flag is painted separately from the description,
/// but the padding is measured from the plain text so ANSI escapes don't throw
/// the description column off.
fn option_row(color: bool, flags: &str, description: &str) {
    const DESCRIPTION_COLUMN: usize = 22;
    let plain = format!("  {flags}");
    let padding = " ".repeat(DESCRIPTION_COLUMN.saturating_sub(plain.len()));
    let rendered = FLAG.paint(&plain, color);
    println!("{rendered}{padding}{description}");
}

/// Print a parse error in the same style as `--help`, followed by the usage
/// line.
fn print_error(message: &str) {
    let color = stderr_color();
    eprintln!("{} {message}", ERROR.paint("error:", color));
    eprintln!(
        "{} {}",
        HEADING.paint("Usage:", color),
        ACCENT.paint(USAGE, color)
    );
}

/// Emit a daemon status line. The prefix is painted on stderr only, so log
/// files stay plain.
pub fn info(args: fmt::Arguments<'_>) {
    eprintln!("{} {args}", ACCENT.paint("hyprview:", stderr_color()));
}

/// Emit a recoverable warning, painted amber.
pub fn warn(args: fmt::Arguments<'_>) {
    eprintln!("{} {args}", WARNING.paint("hyprview:", stderr_color()));
}

/// Emit a fatal or user-actionable error, painted red.
pub fn error(args: fmt::Arguments<'_>) {
    eprintln!("{} {args}", ERROR.paint("hyprview:", stderr_color()));
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub toggle: bool,
    pub no_preview: bool,
    pub allow_mouse: bool,
}

/// What the parser decided the arguments mean. `Help` and `Version` are not
/// errors: they're requests to print and exit successfully.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Options(Options),
    Help,
    Version,
}

/// A command-line parse failure. A typed error rather than a `String` keeps the
/// messages testable and lets the caller choose how to render them.
#[derive(Debug, PartialEq, Eq)]
enum Error {
    UnknownOption(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::UnknownOption(option) => write!(f, "unknown option: {option}"),
        }
    }
}

/// Parse arguments (excluding the program name).
///
/// Returning a result instead of printing and exiting keeps this testable; the
/// thin [`options`] wrapper below handles the exit.
fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Outcome, Error> {
    let mut options = Options::default();
    for arg in args {
        match arg.as_str() {
            "--toggle" => options.toggle = true,
            "--no-preview" => options.no_preview = true,
            "--allow-mouse" => options.allow_mouse = true,
            "--help" | "-h" => return Ok(Outcome::Help),
            "--version" | "-V" => return Ok(Outcome::Version),
            unknown => return Err(Error::UnknownOption(unknown.to_owned())),
        }
    }
    Ok(Outcome::Options(options))
}

/// Parse the process arguments, printing help, version, or an error and
/// exiting as appropriate.
pub fn options() -> Options {
    match parse_args(env::args().skip(1)) {
        Ok(Outcome::Options(options)) => options,
        Ok(Outcome::Help) => {
            print_help();
            std::process::exit(0);
        }
        Ok(Outcome::Version) => {
            print_version();
            std::process::exit(0);
        }
        Err(error) => {
            print_error(&error.to_string());
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Outcome, Error> {
        parse_args(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn parses_no_arguments() {
        assert_eq!(parse(&[]), Ok(Outcome::Options(Options::default())));
    }

    #[test]
    fn parses_every_flag() {
        let outcome = parse(&["--toggle", "--no-preview", "--allow-mouse"]).unwrap();
        assert_eq!(
            outcome,
            Outcome::Options(Options {
                toggle: true,
                no_preview: true,
                allow_mouse: true,
            })
        );
    }

    #[test]
    fn parses_help_aliases() {
        assert_eq!(parse(&["--help"]), Ok(Outcome::Help));
        assert_eq!(parse(&["-h"]), Ok(Outcome::Help));
    }

    #[test]
    fn parses_version_aliases() {
        assert_eq!(parse(&["--version"]), Ok(Outcome::Version));
        assert_eq!(parse(&["-V"]), Ok(Outcome::Version));
    }

    #[test]
    fn rejects_unknown_option() {
        assert_eq!(
            parse(&["--nope"]),
            Err(Error::UnknownOption("--nope".to_owned()))
        );
        assert_eq!(
            Error::UnknownOption("--nope".to_owned()).to_string(),
            "unknown option: --nope"
        );
    }

    #[test]
    fn paint_is_plain_when_colour_is_disabled() {
        assert_eq!(ACCENT.paint("hyprview:", false), "hyprview:");
        assert_eq!(wordmark(false), "hyprview");
        assert_eq!(
            version_line_with(None, false),
            format!("hyprview {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            version_line_with(Some("a5a668f"), false),
            format!("hyprview {} (a5a668f)", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(
            version_line_with(Some(""), false),
            format!("hyprview {}", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn paint_emits_ansi_when_enabled() {
        assert_eq!(
            ACCENT.paint("x", true),
            "\x1b[1m\x1b[38;2;143;168;255mx\x1b[0m"
        );
        assert_eq!(wordmark(true).matches("\x1b[38;2;").count(), NAME.len());
    }
}
