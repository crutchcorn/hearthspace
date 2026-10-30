use std::ffi::OsString;

use crate::state::SettingsCategory;

pub(crate) const USAGE: &str = "Usage: hearthspace-settings [--page PAGE | --category PAGE]

Pages: appearance (default), displays, background, gestures, shortcuts, ai, system
Aliases: display, search, advanced
Options: --help, -h";

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Startup {
    Page(SettingsCategory),
    Help,
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Startup, String> {
    let mut args = args.into_iter();
    let Some(argument) = args.next() else {
        return Ok(Startup::Page(SettingsCategory::Appearance));
    };
    let argument = argument
        .to_str()
        .ok_or_else(|| format!("Arguments must be valid UTF-8.\n{USAGE}"))?;
    let startup = match argument {
        "--help" | "-h" => Startup::Help,
        "--page" | "--category" => {
            let page = args
                .next()
                .ok_or_else(|| format!("{argument} requires a page name.\n{USAGE}"))?;
            Startup::Page(parse_page(page.to_str().unwrap_or_default())?)
        }
        argument => match argument
            .strip_prefix("--page=")
            .or_else(|| argument.strip_prefix("--category="))
        {
            Some(page) => Startup::Page(parse_page(page)?),
            None => return Err(format!("Unknown argument: {argument}.\n{USAGE}")),
        },
    };
    if let Some(argument) = args.next() {
        return Err(format!("Unexpected argument: {argument:?}.\n{USAGE}"));
    }
    Ok(startup)
}

pub(crate) fn parse_page(page: &str) -> Result<SettingsCategory, String> {
    match page {
        "display" | "displays" => Ok(SettingsCategory::Displays),
        "appearance" => Ok(SettingsCategory::Appearance),
        "background" => Ok(SettingsCategory::Background),
        "search" | "gestures" => Ok(SettingsCategory::Gestures),
        "shortcuts" | "keyboard-shortcuts" | "keyboard shortcuts" => {
            Ok(SettingsCategory::KeyboardShortcuts)
        }
        "ai" => Ok(SettingsCategory::Ai),
        "advanced" | "system" => Ok(SettingsCategory::System),
        _ => Err(format!("Unknown settings page: {page:?}.\n{USAGE}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments(args: &[&str]) -> Result<Startup, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn ordinary_launch_starts_on_appearance() {
        assert_eq!(
            arguments(&[]),
            Ok(Startup::Page(SettingsCategory::Appearance))
        );
    }

    #[test]
    fn named_pages_select_the_requested_category() {
        for (name, category) in [
            ("display", SettingsCategory::Displays),
            ("displays", SettingsCategory::Displays),
            ("background", SettingsCategory::Background),
            ("appearance", SettingsCategory::Appearance),
            ("search", SettingsCategory::Gestures),
            ("gestures", SettingsCategory::Gestures),
            ("shortcuts", SettingsCategory::KeyboardShortcuts),
            ("keyboard-shortcuts", SettingsCategory::KeyboardShortcuts),
            ("keyboard shortcuts", SettingsCategory::KeyboardShortcuts),
            ("ai", SettingsCategory::Ai),
            ("advanced", SettingsCategory::System),
            ("system", SettingsCategory::System),
        ] {
            assert_eq!(arguments(&["--page", name]), Ok(Startup::Page(category)));
            assert_eq!(
                arguments(&["--category", name]),
                Ok(Startup::Page(category))
            );
            assert_eq!(
                arguments(&[&format!("--page={name}")]),
                Ok(Startup::Page(category))
            );
            assert_eq!(
                arguments(&[&format!("--category={name}")]),
                Ok(Startup::Page(category))
            );
        }
    }

    #[test]
    fn help_does_not_start_a_window() {
        assert_eq!(arguments(&["--help"]), Ok(Startup::Help));
        assert_eq!(arguments(&["-h"]), Ok(Startup::Help));
    }

    #[test]
    fn invalid_or_ambiguous_arguments_are_rejected() {
        for args in [
            vec!["--page"],
            vec!["--page", "favorites"],
            vec!["--page="],
            vec!["--category"],
            vec!["--category="],
            vec!["search"],
            vec!["--unknown"],
            vec!["--page", "search", "--page", "display"],
            vec!["--page", "shortcuts", "--category", "system"],
            vec!["--help", "--unknown"],
        ] {
            assert!(arguments(&args).is_err(), "accepted {args:?}");
        }
    }
}
