//! Settings owns app gesture editing. Recording never launches an application.
//! The legacy favorites store remains the compatible persistence format.
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
            Err(error) => (None, Some(format!("Gestures are unavailable: {error}"))),
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
                self.message = Some("Gestures reloaded.".into());
            }
            Err(error) => {
                self.store = None;
                self.message = Some(format!("Gestures are unavailable: {error}"));
            }
        }
    }

    pub fn entries(&self) -> &[Favorite] {
        self.store.as_ref().map_or(&[], FavoriteStore::entries)
    }

    /// Old entries with no gesture have no visible meaning in Settings.
    pub fn assigned(&self) -> impl Iterator<Item = &Favorite> {
        self.entries()
            .iter()
            .filter(|entry| entry.gesture.is_some())
    }

    pub fn name(&self, id: &str) -> String {
        self.catalog
            .app_by_id(id)
            .map_or_else(|| id.to_owned(), |app| app.name.clone())
    }

    pub fn remove_gesture(&mut self, id: &str) {
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
                    self.message = Some(format!("Removed the gesture for {name}."));
                }
                Err(error) => self.message = Some(error.to_string()),
            }
        }
    }

    pub fn start(&mut self, id: &str) {
        if self.store.is_none() {
            return;
        }
        if self.catalog.app_by_id(id).is_none() {
            self.message = Some("Choose an installed app first.".into());
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
        match store.set_gesture(&id, &recording.points) {
            Ok(()) => {
                self.cancel();
                self.message = Some(format!(
                    "Saved the gesture for {name}. Draw it in search to launch the app."
                ));
            }
            Err(error) => self.message = Some(error.to_string()),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
