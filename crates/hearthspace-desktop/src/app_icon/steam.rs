//! Narrow, offline fallback for Steam shortcuts declaring the client icon.
use std::{fs, path::Path, path::PathBuf};

use crate::{app_catalog::DesktopApp, artwork::RasterImage};

const MAX_CACHE_ENTRIES: usize = 128;
const STEAM_ROOTS: &[&str] = &[
    ".local/share/Steam",
    ".steam/steam",
    ".steam/root",
    "snap/steam/common/.local/share/Steam",
    ".var/app/com.valvesoftware.Steam/.local/share/Steam",
    ".var/app/com.valvesoftware.Steam/.steam/steam",
];

pub(super) fn installed_roots() -> Vec<PathBuf> {
    dirs::home_dir().map_or_else(Vec::new, |home| roots_under(&home))
}

fn roots_under(home: &Path) -> Vec<PathBuf> {
    STEAM_ROOTS.iter().map(|root| home.join(root)).collect()
}

pub(super) fn game_id(app: &DesktopApp) -> Option<String> {
    let argv = app.launch_argv().ok()?;
    if Path::new(argv.first()?).file_name()? != "steam" {
        return None;
    }
    let mut uris = argv
        .iter()
        .skip(1)
        .filter(|arg| arg.starts_with("steam://"));
    let id = uris.next()?.strip_prefix("steam://rungameid/")?;
    // A second URI is ambiguous. Bound the numeric component before using it
    // as a path; reject signs, whitespace, traversal, queries and URL escapes.
    if uris.next().is_some()
        || id.is_empty()
        || id.len() > 20
        || !id.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    Some(id.to_owned())
}

pub(super) fn cached_icon(roots: &[PathBuf], id: &str) -> Option<RasterImage> {
    // A stale legacy file in one installation must not mask modern artwork in
    // another (for example, native Steam followed by the active Snap install).
    roots
        .iter()
        .find_map(|root| {
            unique_icon(&root.join("appcache/librarycache").join(id))
                .and_then(|path| square_icon(&path))
        })
        .or_else(|| {
            roots.iter().find_map(|root| {
                square_icon(
                    &root
                        .join("appcache/librarycache")
                        .join(format!("{id}_icon.jpg")),
                )
            })
        })
}

fn square_icon(path: &Path) -> Option<RasterImage> {
    super::decode_file_with_shape(path, true)
}

fn unique_icon(directory: &Path) -> Option<PathBuf> {
    let mut candidate = None;
    // Inspect only this game's immediate cache. Do not parse appinfo, recurse
    // through accounts, or mistake named headers/capsules for game icons.
    for (index, entry) in fs::read_dir(directory).ok()?.enumerate() {
        if index >= MAX_CACHE_ENTRIES {
            return None;
        }
        let entry = entry.ok()?;
        let name = entry.file_name();
        let Some(hash) = name.to_str().and_then(|name| name.strip_suffix(".jpg")) else {
            continue;
        };
        if hash.len() == 40 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            if candidate.is_some() || !entry.file_type().ok()?.is_file() {
                return None;
            }
            candidate = Some(entry.path());
        }
    }
    candidate
}

#[cfg(test)]
mod tests;
