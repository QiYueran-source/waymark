//! Work-state colour for a terminal.
//!
//! The domain owns the display-state derivation ([`waymark_core::view::DisplayState`]);
//! this adapter only decides whether a human line may carry ANSI and which SGR
//! code approximates the Web's semantic colour. JSON envelopes, `show --raw`
//! documents, and raw `git diff` patches never pass through here.

use std::ffi::OsStr;
use std::io::IsTerminal;

use clap::ValueEnum;
use waymark_core::view::DisplayState;

/// When human output may carry ANSI colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColourChoice {
    /// Colour only when the human stream is a terminal and `NO_COLOR` is unset.
    Auto,
    /// Always colour, even through a pipe; this overrides `NO_COLOR`.
    Always,
    /// Never colour; the text is byte-identical to the plain output.
    Never,
}

/// Whether the resolved flags and environment allow colour.
///
/// `NO_COLOR` suppresses `auto` only: an explicit `--color=always` is the
/// caller asking for colour and wins over the environment, while `never` is
/// always plain.
pub fn enabled(choice: ColourChoice, no_color: bool, stream_is_tty: bool) -> bool {
    match choice {
        ColourChoice::Always => true,
        ColourChoice::Never => false,
        ColourChoice::Auto => !no_color && stream_is_tty,
    }
}

/// `NO_COLOR` disables colour only when it is present and non-empty.
///
/// This is the no-color.org convention: a merely present but empty variable is
/// not a request to disable colour.
pub fn no_color_is_set(value: Option<&OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
}

fn no_color_from_env() -> bool {
    no_color_is_set(std::env::var_os("NO_COLOR").as_deref())
}

/// The two renditions of one human line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Palette {
    /// No escapes; the byte-identical human output of today.
    Plain,
    /// ANSI SGR approximating the Web's semantic work-state colours.
    Ansi,
}

impl Palette {
    /// Resolve the palette for human output from the global flag.
    ///
    /// Human prose goes to stderr, so stderr is the stream whose terminal
    /// capability decides `auto`; stdout carries JSON, `show --raw` documents,
    /// and raw `git diff` patches, which are never coloured.
    pub fn for_human_stream(choice: ColourChoice) -> Self {
        if enabled(choice, no_color_from_env(), std::io::stderr().is_terminal()) {
            Self::Ansi
        } else {
            Self::Plain
        }
    }

    /// Paint `text` for `state`, or return it unchanged in plain mode.
    pub fn paint(self, state: DisplayState, text: &str) -> String {
        match self {
            Self::Plain => text.to_owned(),
            Self::Ansi => format!("{}{text}{RESET}", sgr(state)),
        }
    }
}

/// ANSI SGR reset.
const RESET: &str = "\u{1b}[0m";

/// The ANSI approximation of the Web's green/blue/red/grey work-state colours.
///
/// The Web's hex values are not a shared contract; only the semantic mapping
/// is. These are the standard 16-colour SGR equivalents: green, blue, red, and
/// bright black as grey.
fn sgr(state: DisplayState) -> &'static str {
    match state {
        DisplayState::Ready => "\u{1b}[32m",
        DisplayState::InProgress => "\u{1b}[34m",
        DisplayState::Blocked => "\u{1b}[31m",
        DisplayState::Closed => "\u{1b}[90m",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_colours_only_a_terminal_without_no_color() {
        assert!(enabled(ColourChoice::Auto, false, true));
        assert!(!enabled(ColourChoice::Auto, false, false));
        assert!(!enabled(ColourChoice::Auto, true, true));
        assert!(!enabled(ColourChoice::Auto, true, false));
    }

    #[test]
    fn never_is_plain_even_on_a_terminal() {
        assert!(!enabled(ColourChoice::Never, false, true));
        assert!(!enabled(ColourChoice::Never, false, false));
    }

    #[test]
    fn always_forces_colour_on_a_pipe_and_overrides_no_color() {
        assert!(enabled(ColourChoice::Always, true, false));
        assert!(enabled(ColourChoice::Always, false, false));
        assert!(enabled(ColourChoice::Always, true, true));
    }

    #[test]
    fn no_color_counts_only_when_it_is_present_and_non_empty() {
        assert!(no_color_is_set(Some(OsStr::new("1"))));
        assert!(no_color_is_set(Some(OsStr::new("anything"))));
        assert!(!no_color_is_set(Some(OsStr::new(""))));
        assert!(!no_color_is_set(None));
    }

    #[test]
    fn each_display_state_uses_the_web_semantic_colour() {
        assert_eq!(
            Palette::Ansi.paint(DisplayState::Ready, "ready"),
            "\u{1b}[32mready\u{1b}[0m"
        );
        assert_eq!(
            Palette::Ansi.paint(DisplayState::InProgress, "ready"),
            "\u{1b}[34mready\u{1b}[0m"
        );
        assert_eq!(
            Palette::Ansi.paint(DisplayState::Blocked, "blocked"),
            "\u{1b}[31mblocked\u{1b}[0m"
        );
        assert_eq!(
            Palette::Ansi.paint(DisplayState::Closed, "-"),
            "\u{1b}[90m-\u{1b}[0m"
        );
    }

    #[test]
    fn a_plain_palette_changes_nothing() {
        assert_eq!(
            Palette::Plain.paint(DisplayState::Blocked, "blocked"),
            "blocked"
        );
    }
}
