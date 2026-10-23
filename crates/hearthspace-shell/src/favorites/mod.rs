//! Persistent favorites and deterministic, conservative single-stroke matching.

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

mod gesture;
mod storage;

use gesture::{AMBIGUITY_MARGIN, MATCH_DISTANCE};
pub use gesture::{Gesture, MAX_STROKE_POINTS, Point};

pub const MAX_FAVORITES: usize = 24;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Favorite {
    /// The installed application's desktop-file ID; never an executable command.
    pub app_id: String,
    pub gesture: Option<Gesture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recognition {
    Match(String),
    NoMatch,
    Ambiguous,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FavoriteError(pub String);

impl fmt::Display for FavoriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FavoriteError {}

#[derive(Clone, Debug)]
pub struct FavoriteStore {
    entries: Vec<Favorite>,
    path: Option<PathBuf>,
    /// Detect another shell or editor's changes before replacing the file.
    disk_contents: Option<Vec<u8>>,
}

impl FavoriteStore {
    pub fn in_memory() -> Self {
        Self {
            entries: Vec::new(),
            path: None,
            disk_contents: None,
        }
    }

    pub fn load_default() -> Result<Self, FavoriteError> {
        let path = hearthspace_common::config_path()
            .map_err(|e| FavoriteError(format!("Cannot locate favorites: {e}")))?
            .with_file_name("favorites.json");
        Self::load(path)
    }

    /// Missing files start empty. Malformed/unreadable files return an error and
    /// must not be replaced by an empty fallback store at the same path.
    pub fn load(path: impl Into<PathBuf>) -> Result<Self, FavoriteError> {
        storage::load(path.into())
    }

    pub fn entries(&self) -> &[Favorite] {
        &self.entries
    }

    /// Observe changes from Settings without rewriting anything. Malformed or
    /// unreadable replacements leave the last successfully loaded favorites intact.
    pub fn reload_if_changed(&mut self) -> Result<bool, FavoriteError> {
        let Some(path) = &self.path else {
            return Ok(false);
        };
        let updated = Self::load(path.clone())?;
        if updated.disk_contents == self.disk_contents {
            return Ok(false);
        }
        *self = updated;
        Ok(true)
    }

    pub fn add(&mut self, app_id: &str) -> Result<(), FavoriteError> {
        validate_id(app_id)?;
        if self.entries.iter().any(|entry| entry.app_id == app_id) {
            return Ok(());
        }
        if self.entries.len() == MAX_FAVORITES {
            return Err(FavoriteError(format!(
                "You can save up to {MAX_FAVORITES} favorite apps."
            )));
        }
        let mut entries = self.entries.clone();
        entries.push(Favorite {
            app_id: app_id.to_owned(),
            gesture: None,
        });
        self.replace(entries)
    }

    pub fn assign(&mut self, app_id: &str, points: &[Point]) -> Result<(), FavoriteError> {
        let gesture = Gesture::from_points(points)?;
        let index = self.index(app_id)?;
        self.validate_gesture(app_id, &gesture)?;
        let mut entries = self.entries.clone();
        entries[index].gesture = Some(gesture);
        self.replace(entries)
    }

    /// Save an app and its gesture in one transaction, retaining the order and
    /// membership of favorites that launch without a drawing.
    pub fn set_gesture(&mut self, app_id: &str, points: &[Point]) -> Result<(), FavoriteError> {
        validate_id(app_id)?;
        let gesture = Gesture::from_points(points)?;
        self.validate_gesture(app_id, &gesture)?;
        let mut entries = self.entries.clone();
        if let Some(entry) = entries.iter_mut().find(|entry| entry.app_id == app_id) {
            entry.gesture = Some(gesture);
        } else {
            if entries.len() >= MAX_FAVORITES {
                return Err(FavoriteError(format!(
                    "You can save up to {MAX_FAVORITES} favorite apps, with or without gestures."
                )));
            }
            entries.push(Favorite {
                app_id: app_id.to_owned(),
                gesture: Some(gesture),
            });
        }
        self.replace(entries)
    }

    /// Retarget one saved shortcut only when the complete replacement can be
    /// committed. Keep the original app pinned in its saved position; a
    /// different app's existing shortcut is never overwritten.
    pub fn replace_gesture_app(
        &mut self,
        original_id: &str,
        app_id: &str,
        points: &[Point],
    ) -> Result<(), FavoriteError> {
        if original_id == app_id {
            return self.set_gesture(app_id, points);
        }
        let original = self.index(original_id)?;
        if self.entries[original].gesture.is_none() {
            return Err(FavoriteError(
                "This app has no saved gesture to edit.".into(),
            ));
        }
        if self
            .entries
            .iter()
            .any(|entry| entry.app_id == app_id && entry.gesture.is_some())
        {
            return Err(FavoriteError(
                "This app already has a gesture. Choose another app.".into(),
            ));
        }
        // Reuse upsert validation against every other assigned shape. Building
        // an in-memory candidate leaves both disk and live state untouched if
        // validation or the final optimistic write fails.
        let mut candidate = Self::in_memory();
        candidate.entries = self.entries.clone();
        candidate.entries[original].gesture = None;
        candidate.set_gesture(app_id, points)?;
        self.replace(candidate.entries)
    }

    fn validate_gesture(&self, app_id: &str, gesture: &Gesture) -> Result<(), FavoriteError> {
        for entry in &self.entries {
            if entry.app_id != app_id
                && entry
                    .gesture
                    .as_ref()
                    .is_some_and(|other| gesture.distance_to(other) <= MATCH_DISTANCE)
            {
                return Err(FavoriteError(format!(
                    "This shape is too similar to the shape for {}. Draw a different shape.",
                    entry.app_id
                )));
            }
        }
        Ok(())
    }

    pub fn clear_gesture(&mut self, app_id: &str) -> Result<(), FavoriteError> {
        let index = self.index(app_id)?;
        let mut entries = self.entries.clone();
        entries[index].gesture = None;
        self.replace(entries)
    }

    pub fn remove(&mut self, app_id: &str) -> Result<(), FavoriteError> {
        let index = self.index(app_id)?;
        let mut entries = self.entries.clone();
        entries.remove(index);
        self.replace(entries)
    }

    pub fn recognize(&self, points: &[Point]) -> Result<Recognition, FavoriteError> {
        let gesture = Gesture::from_points(points)?;
        let mut candidates: Vec<_> = self
            .entries
            .iter()
            .filter_map(|entry| {
                entry
                    .gesture
                    .as_ref()
                    .map(|template| (gesture.distance_to(template), &entry.app_id))
            })
            .collect();
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)));
        let Some((best, app_id)) = candidates.first() else {
            return Ok(Recognition::NoMatch);
        };
        if *best > MATCH_DISTANCE {
            return Ok(Recognition::NoMatch);
        }
        if candidates
            .get(1)
            .is_some_and(|runner_up| runner_up.0 - best < AMBIGUITY_MARGIN)
        {
            return Ok(Recognition::Ambiguous);
        }
        Ok(Recognition::Match((*app_id).clone()))
    }

    fn index(&self, app_id: &str) -> Result<usize, FavoriteError> {
        self.entries
            .iter()
            .position(|entry| entry.app_id == app_id)
            .ok_or_else(|| FavoriteError("Add this app to favorites first.".to_owned()))
    }

    fn replace(&mut self, entries: Vec<Favorite>) -> Result<(), FavoriteError> {
        if let Some(path) = &self.path {
            self.disk_contents = Some(storage::save(
                path,
                &entries,
                self.disk_contents.as_deref(),
            )?);
        }
        self.entries = entries;
        Ok(())
    }
}

fn validate_id(id: &str) -> Result<(), FavoriteError> {
    if id.is_empty() || id.len() > 512 || id.chars().any(char::is_control) {
        return Err(FavoriteError("The application ID is invalid.".to_owned()));
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

#[cfg(test)]
mod upsert_tests;
