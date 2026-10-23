use std::path::Path;

use super::{AppCatalog, DesktopApp, exec::split_exec};

impl AppCatalog {
    /// Match a Wayland app ID or X11 WM_CLASS to its installed desktop entry.
    /// Explicit metadata takes priority over whitespace-compacted desktop IDs
    /// and executable-name fallbacks. Equally ranked matches are ambiguous.
    pub fn app_for_window(&self, identity: &str) -> Option<&DesktopApp> {
        if identity.is_empty() {
            return None;
        }
        let mut best = None;
        let mut best_rank = u8::MAX;
        for app in &self.apps {
            let Some(rank) = match_rank(app, identity) else {
                continue;
            };
            if rank < best_rank {
                best = Some(app);
                best_rank = rank;
            } else if rank == best_rank {
                // An ambiguous stronger match must not fall through to a weaker
                // executable match, such as a game launched by the Steam client.
                best = None;
            }
        }
        best
    }
}

fn match_rank(app: &DesktopApp, identity: &str) -> Option<u8> {
    let id = app.id.strip_suffix(".desktop").unwrap_or(&app.id);
    // `.desktop` may be part of the actual app ID (for example,
    // ai.opencode.desktop). Strip only the entry's filename extension and
    // prefer that literal identity over a caller-supplied desktop filename.
    if id == identity {
        return Some(0);
    }
    if app.id == identity {
        return Some(1);
    }
    if id.eq_ignore_ascii_case(identity) {
        return Some(2);
    }
    if app.id.eq_ignore_ascii_case(identity) {
        return Some(3);
    }
    if app
        .startup_wm_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case(identity))
    {
        return Some(4);
    }
    if [&app.snap_app_name, &app.snap_common_id]
        .into_iter()
        .flatten()
        .any(|alias| alias.eq_ignore_ascii_case(identity))
    {
        return Some(5);
    }
    // Some clients omit whitespace from their desktop ID in WM_CLASS (for
    // example, PepperGrinder vs Pepper Grinder.desktop). Keep punctuation and
    // literal .desktop suffixes intact, and rank this below explicit metadata.
    // The catalog's tie handling rejects collisions before trying executables.
    if compact_id_matches(id, identity) {
        return Some(6);
    }
    if compact_id_matches(&app.id, identity) {
        return Some(7);
    }
    let argv = split_exec(&app.exec).ok()?;
    argv.first()
        .and_then(|command| Path::new(command).file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(identity))
        .then_some(8)
}

fn compact_id_matches(id: &str, identity: &str) -> bool {
    id.chars()
        .filter(|ch| !ch.is_whitespace())
        .map(|ch| ch.to_ascii_lowercase())
        .eq(identity.chars().map(|ch| ch.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod compact_tests;
