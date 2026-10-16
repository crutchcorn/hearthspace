use std::ffi::OsString;

use crate::state::SettingsCategory;

pub(crate) const USAGE: &str = "Usage: hearthspace-settings [--page PAGE]

Pages: display (default), appearance, search, ai, advanced
Options: --help, -h";

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Startup {
    Page(SettingsCategory),
    Help,
}

pub(crate) fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Startup, String> {
    let mut args = args.into_iter();
    let Some(argument) = args.next() else {
        return Ok(Startup::Page(SettingsCategory::Display));
    };
    let argument = argument
        .to_str()
        .ok_or_else(|| format!("Arguments must be valid UTF-8.\n{USAGE}"))?;
    let startup = match argument {
        "--help" | "-h" => Startup::Help,
        "--page" => {
            let page = args
                .next()
                .ok_or_else(|| format!("--page requires a page name.\n{USAGE}"))?;
            Startup::Page(parse_page(page.to_str().unwrap_or_default())?)
        }
        argument => match argument.strip_prefix("--page=") {
            Some(page) => Startup::Page(parse_page(page)?),
            None => return Err(format!("Unknown argument: {argument}.\n{USAGE}")),
        },
    };
    if let Some(argument) = args.next() {
        return Err(format!("Unexpected argument: {argument:?}.\n{USAGE}"));
    }
    Ok(startup)
}

fn parse_page(page: &str) -> Result<SettingsCategory, String> {
    match page {
        "display" => Ok(SettingsCategory::Display),
        "appearance" => Ok(SettingsCategory::Appearance),
        "search" => Ok(SettingsCategory::Search),
        "ai" => Ok(SettingsCategory::Ai),
        "advanced" => Ok(SettingsCategory::Advanced),
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
    fn ordinary_launch_starts_on_display() {
        assert_eq!(arguments(&[]), Ok(Startup::Page(SettingsCategory::Display)));
    }

    #[test]
    fn named_pages_select_the_requested_category() {
        for (name, category) in [
            ("display", SettingsCategory::Display),
            ("appearance", SettingsCategory::Appearance),
            ("search", SettingsCategory::Search),
            ("ai", SettingsCategory::Ai),
            ("advanced", SettingsCategory::Advanced),
        ] {
            assert_eq!(arguments(&["--page", name]), Ok(Startup::Page(category)));
            assert_eq!(
                arguments(&[&format!("--page={name}")]),
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
            vec!["search"],
            vec!["--unknown"],
            vec!["--page", "search", "--page", "display"],
            vec!["--help", "--unknown"],
        ] {
            assert!(arguments(&args).is_err(), "accepted {args:?}");
        }
    }
}
