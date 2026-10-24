//! Favorites are shared between outputs; an intentional drawing session is not.
use hearthspace_desktop::favorites::{FavoriteStore, Point};
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct Favorites {
    pub store: FavoriteStore,
    pub error: Option<String>,
    /// Runtime instances refresh installed apps; isolated test states never scan
    /// the host catalog when their in-memory favorites change.
    pub tracks_installed_apps: bool,
    pending_path: Option<std::path::PathBuf>,
}

impl Default for Favorites {
    fn default() -> Self {
        Self {
            store: FavoriteStore::in_memory(),
            error: None,
            tracks_installed_apps: false,
            pending_path: None,
        }
    }
}

impl Favorites {
    pub fn load() -> Self {
        match hearthspace_common::config_path() {
            Ok(path) => Self::load_path(path.with_file_name("favorites.json")),
            Err(error) => Self {
                error: Some(error.to_string()),
                tracks_installed_apps: true,
                ..Self::default()
            },
        }
    }

    fn load_path(path: std::path::PathBuf) -> Self {
        match FavoriteStore::load(&path) {
            Ok(store) => Self {
                store,
                error: None,
                tracks_installed_apps: true,
                pending_path: None,
            },
            Err(error) => Self {
                error: Some(error.to_string()),
                tracks_installed_apps: true,
                pending_path: Some(path),
                ..Self::default()
            },
        }
    }

    pub fn refresh(&mut self) -> bool {
        if let Some(path) = self.pending_path.clone() {
            *self = Self::load_path(path);
            return self.error.is_none();
        }
        match self.store.reload_if_changed() {
            Ok(changed) => {
                self.error = None;
                changed
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) enum Mode {
    #[default]
    Browse,
    Launch,
}

#[derive(Debug)]
pub(super) struct Incoming {
    pub stroke: u64,
    pub next_sequence: u64,
    pub points: Vec<Point>,
}

#[derive(Debug, Default)]
pub(super) struct Drawing {
    pub mode: Mode,
    pub generation: u64,
    pub bounds: Option<[f64; 4]>,
    pub armed: bool,
    pub arm_attempts: u8,
    pub arm_retry_deadline: Option<std::time::Instant>,
    pub incoming: Option<Incoming>,
    pub preview: Vec<Point>,
    pub message: Option<String>,
}

impl Drawing {
    pub fn start(&mut self, mode: Mode) {
        *self = Self {
            mode,
            ..Self::default()
        };
        self.refresh_generation();
    }
    pub fn active(&self) -> bool {
        self.mode != Mode::Browse
    }

    pub fn refresh_generation(&mut self) {
        static GENERATION: AtomicU64 = AtomicU64::new(1);
        self.generation = GENERATION.fetch_add(1, Ordering::Relaxed);
        self.armed = false;
        self.arm_attempts = 0;
        self.arm_retry_deadline = None;
        self.incoming = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repaired_settings_file_recovers_from_initial_load_error() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("favorites.json");
        std::fs::write(&path, "malformed").unwrap();
        let mut favorites = Favorites::load_path(path.clone());
        assert!(favorites.tracks_installed_apps);
        assert!(favorites.error.is_some());
        assert!(!favorites.refresh());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "malformed");
        std::fs::remove_file(&path).unwrap();
        let mut settings = FavoriteStore::load(&path).unwrap();
        settings.add("editor.desktop").unwrap();
        assert!(favorites.refresh());
        assert!(favorites.error.is_none());
        assert_eq!(favorites.store.entries(), settings.entries());
        assert!(!favorites.refresh());
    }

    #[test]
    fn default_state_refresh_preserves_in_memory_favorites_without_disk_access() {
        let mut favorites = Favorites::default();
        assert!(!favorites.tracks_installed_apps);
        favorites.store.add("editor.desktop").unwrap();
        assert!(!favorites.refresh());
        assert_eq!(favorites.store.entries().len(), 1);
    }
}
