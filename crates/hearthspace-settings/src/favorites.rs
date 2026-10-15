//! Settings owns favorite editing. Recording never launches an application.
use hearthspace_shell::{
    app_catalog::AppCatalog,
    favorites::{Favorite, FavoriteError, FavoriteStore, Gesture, Point},
};

pub(crate) struct FavoriteSettings {
    pub store: Option<FavoriteStore>,
    pub catalog: AppCatalog,
    pub query: String,
    pub recording: Option<Recording>,
    pub message: Option<String>,
    generation: u64,
}

pub(crate) struct Recording {
    pub app_id: String,
    pub generation: u64,
    pub points: Vec<Point>,
    pub valid: bool,
}

impl FavoriteSettings {
    pub fn load() -> Self {
        Self::from_source(FavoriteStore::load_default(), AppCatalog::load())
    }

    pub(crate) fn from_source(
        store: Result<FavoriteStore, FavoriteError>,
        catalog: AppCatalog,
    ) -> Self {
        let (store, message) = match store {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(format!("Favorites are unavailable: {error}"))),
        };
        Self {
            store,
            catalog,
            query: String::new(),
            recording: None,
            message,
            generation: 0,
        }
    }

    pub fn reload(&mut self) {
        self.cancel();
        self.catalog = AppCatalog::load();
        match FavoriteStore::load_default() {
            Ok(store) => {
                self.store = Some(store);
                self.message = Some("Favorites reloaded.".into());
            }
            Err(error) => {
                self.store = None;
                self.message = Some(format!("Favorites are unavailable: {error}"));
            }
        }
    }

    pub fn entries(&self) -> &[Favorite] {
        self.store.as_ref().map_or(&[], FavoriteStore::entries)
    }

    pub fn name(&self, id: &str) -> String {
        self.catalog
            .app_by_id(id)
            .map_or_else(|| id.to_owned(), |app| app.name.clone())
    }

    pub fn add(&mut self, id: &str) {
        if self.catalog.app_by_id(id).is_none() {
            self.message = Some("This app is no longer installed. Reload the app list.".into());
            return;
        }
        let name = self.name(id);
        if let Some(store) = &mut self.store {
            self.message = Some(match store.add(id) {
                Ok(()) => format!("Added {name} to favorites."),
                Err(error) => error.to_string(),
            });
        }
    }

    pub fn remove(&mut self, id: &str) {
        let name = self.name(id);
        if let Some(store) = &mut self.store {
            match store.remove(id) {
                Ok(()) => {
                    if self
                        .recording
                        .as_ref()
                        .is_some_and(|recording| recording.app_id == id)
                    {
                        self.cancel();
                    }
                    self.message = Some(format!("Removed {name} from favorites."));
                }
                Err(error) => self.message = Some(error.to_string()),
            }
        }
    }

    pub fn clear(&mut self, id: &str) {
        let name = self.name(id);
        if let Some(store) = &mut self.store {
            self.message = Some(match store.clear_gesture(id) {
                Ok(()) => format!("Removed the gesture for {name}."),
                Err(error) => error.to_string(),
            });
        }
    }

    pub fn start(&mut self, id: &str) {
        if self.catalog.app_by_id(id).is_none()
            || !self.entries().iter().any(|entry| entry.app_id == id)
        {
            self.message = Some("Choose an installed favorite app first.".into());
            return;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        self.recording = Some(Recording {
            app_id: id.to_owned(),
            generation: self.generation,
            points: vec![],
            valid: false,
        });
        self.message = Some("Draw a single shape, then save it when it looks right.".into());
    }

    pub fn stroke(&mut self, app_id: &str, generation: u64, points: Vec<Point>) {
        let Some(recording) = &mut self.recording else {
            return;
        };
        if recording.app_id != app_id || recording.generation != generation {
            return;
        }
        // The drawing view bounds input; validate again before enabling Save.
        let result = Gesture::from_points(&points);
        recording.valid = result.is_ok();
        recording.points = if points.len() <= hearthspace_shell::favorites::MAX_STROKE_POINTS {
            points
        } else {
            vec![]
        };
        self.message = Some(match result {
            Ok(_) => "Shape recorded. Save it, or draw again to replace it.".into(),
            Err(error) => error.to_string(),
        });
    }

    pub fn retry(&mut self) {
        if let Some(recording) = &self.recording {
            let id = recording.app_id.clone();
            self.start(&id);
        }
    }

    pub fn cancel(&mut self) {
        self.recording = None;
        self.generation = self.generation.wrapping_add(1).max(1);
    }

    pub fn save(&mut self) {
        let Some(recording) = &self.recording else {
            return;
        };
        if !recording.valid {
            return;
        }
        let id = recording.app_id.clone();
        let name = self.name(&id);
        let Some(store) = &mut self.store else {
            return;
        };
        match store.assign(&id, &recording.points) {
            Ok(()) => {
                self.cancel();
                self.message = Some(format!(
                    "Saved the gesture for {name}. Open search and choose Draw to launch."
                ));
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
