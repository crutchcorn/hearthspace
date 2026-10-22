use std::path::Path;

use super::{AppCatalog, DesktopApp, exec::split_exec};

impl AppCatalog {
    /// Match a Wayland app ID or X11 WM_CLASS to its installed desktop entry.
    /// Explicit desktop identities take priority over executable-name fallbacks.
    pub fn app_for_window(&self, identity: &str) -> Option<&DesktopApp> {
        let identity = identity.strip_suffix(".desktop").unwrap_or(identity);
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
                // Ambiguous explicit metadata must not fall through to a weaker
                // executable match, such as a game launched by the Steam client.
                best = None;
            }
        }
        best
    }
}

fn match_rank(app: &DesktopApp, identity: &str) -> Option<u8> {
    let id = app.id.strip_suffix(".desktop").unwrap_or(&app.id);
    if id == identity {
        return Some(0);
    }
    if id.eq_ignore_ascii_case(identity) {
        return Some(1);
    }
    if app
        .startup_wm_class
        .as_deref()
        .is_some_and(|class| class.eq_ignore_ascii_case(identity))
    {
        return Some(2);
    }
    if [&app.snap_app_name, &app.snap_common_id]
        .into_iter()
        .flatten()
        .any(|alias| alias.eq_ignore_ascii_case(identity))
    {
        return Some(3);
    }
    let argv = split_exec(&app.exec).ok()?;
    argv.first()
        .and_then(|command| Path::new(command).file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(identity))
        .then_some(4)
}

#[cfg(test)]
mod tests;
