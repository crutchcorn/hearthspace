//! Remember spatial preferences, never client contents or commands to relaunch.
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};

use hearthspace_common::MAX_DESKTOP_ITEMS;
use serde::{Deserialize, Serialize};

use super::{App, windows::window_app_id};
use crate::{
    config::{MAX_ZOOM, MIN_ZOOM},
    geometry::CanvasPoint,
};

mod frame;
#[cfg(test)]
mod tests;

const MAX_PLACEMENTS: usize = 128;
// Covers 4096 desktop IDs of 1024 bytes, including JSON escaping and layout metadata.
const MAX_BYTES: u64 = 12 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Placement {
    app_id: String,
    slot: u8,
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct Layout {
    version: u32,
    x: i32,
    y: i32,
    scale: f64,
    placements: Vec<Placement>,
    #[serde(default)]
    desktop: std::collections::BTreeMap<String, CanvasPoint>,
    #[serde(default)]
    frames: std::collections::BTreeMap<String, hearthspace_common::frame_layout::FrameLayout>,
    #[serde(default)]
    allow_hide_tools: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            x: 0,
            y: 0,
            scale: 1.0,
            placements: Vec::new(),
            desktop: Default::default(),
            frames: Default::default(),
            allow_hide_tools: false,
        }
    }
}

impl Layout {
    fn validate(&self) -> Result<(), &'static str> {
        let coordinate = |value: i32| (-1_000_000..=1_000_000).contains(&value);
        if self.version != 1
            || !coordinate(self.x)
            || !coordinate(self.y)
            || !self.scale.is_finite()
            || !(MIN_ZOOM..=MAX_ZOOM).contains(&self.scale)
            || self.placements.len() > MAX_PLACEMENTS
        {
            return Err("invalid layout version, viewport, or length");
        }
        if self.desktop.len() > MAX_DESKTOP_ITEMS
            || self.desktop.iter().any(|(id, point)| {
                id.is_empty() || id.len() > 1024 || !coordinate(point.x) || !coordinate(point.y)
            })
        {
            return Err("invalid desktop placements");
        }
        if self.frames.len() > 32
            || self.frames.iter().any(|(output, layout)| {
                output.is_empty() || output.len() > 256 || layout.validate().is_err()
            })
        {
            return Err("invalid frame layouts");
        }
        let mut keys = std::collections::HashSet::new();
        for item in &self.placements {
            if item.app_id.is_empty()
                || item.app_id.len() > 256
                || item.slot >= 8
                || !coordinate(item.x)
                || !coordinate(item.y)
                || !keys.insert((&item.app_id, item.slot))
            {
                return Err("invalid or duplicate application placement");
            }
        }
        Ok(())
    }
}

pub(super) struct LayoutStore {
    path: Option<PathBuf>,
    layout: Layout,
    // Window IDs are only used inside this process; saved slots are per app.
    claimed: HashMap<u64, (String, u8)>,
}

impl LayoutStore {
    pub(super) fn from_environment(native: bool) -> Self {
        let path = match std::env::var_os("HEARTHSPACE_LAYOUT_PATH") {
            Some(value) if value == "off" => None,
            Some(value) => Some(PathBuf::from(value)),
            None if native => std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
                })
                .map(|path| path.join("hearthspace/layout.json")),
            None => None,
        };
        Self::load(path.filter(|path| path.is_absolute()))
    }

    fn load(path: Option<PathBuf>) -> Self {
        let layout = path.as_ref().and_then(|path| {
            let read = || -> Result<Layout, Box<dyn std::error::Error>> {
                let mut bytes = Vec::new();
                fs::File::open(path)?.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
                if bytes.len() as u64 > MAX_BYTES { return Err("layout exceeds size limit".into()); }
                let layout: Layout = serde_json::from_slice(&bytes)?;
                layout.validate()?;
                Ok(layout)
            };
            match read() {
                Ok(layout) => Some(layout),
                Err(error) => {
                    if path.exists() { tracing::warn!(%error, path = %path.display(), "ignoring invalid saved layout"); }
                    None
                }
            }
        }).unwrap_or_default();
        Self {
            path,
            layout,
            claimed: HashMap::new(),
        }
    }

    pub(super) fn desktop_position(&self, id: &str) -> Option<CanvasPoint> {
        self.layout.desktop.get(id).copied()
    }

    pub(super) fn remember_desktop(&mut self, id: &str, point: CanvasPoint) {
        if !id.is_empty()
            && id.len() <= 1024
            && point.x.abs_diff(0) <= 1_000_000
            && point.y.abs_diff(0) <= 1_000_000
            && (self.layout.desktop.contains_key(id)
                || self.layout.desktop.len() < MAX_DESKTOP_ITEMS)
        {
            self.layout.desktop.insert(id.to_owned(), point);
        }
    }

    pub(super) fn viewport(&self) -> (CanvasPoint, f64) {
        (
            CanvasPoint {
                x: self.layout.x,
                y: self.layout.y,
            },
            self.layout.scale,
        )
    }

    fn claim(&mut self, id: u64, app_id: String) -> Option<CanvasPoint> {
        if self.path.is_none() || app_id.is_empty() || app_id.len() > 256 {
            return None;
        }
        if self.claimed.contains_key(&id) {
            return None;
        }
        let slot = (0..8).find(|slot| {
            !self
                .claimed
                .values()
                .any(|key| key.0 == app_id && key.1 == *slot)
        });
        let slot = slot?;
        self.claimed.insert(id, (app_id.clone(), slot));
        self.layout
            .placements
            .iter()
            .find(|item| item.app_id == app_id && item.slot == slot)
            .map(|item| CanvasPoint {
                x: item.x,
                y: item.y,
            })
    }

    fn remember(&mut self, id: u64, point: CanvasPoint) {
        let Some((app_id, slot)) = self.claimed.get(&id) else {
            return;
        };
        if point.x.abs_diff(0) > 1_000_000 || point.y.abs_diff(0) > 1_000_000 {
            return;
        }
        if let Some(index) = self
            .layout
            .placements
            .iter()
            .position(|item| item.app_id == *app_id && item.slot == *slot)
        {
            self.layout.placements.remove(index);
        }
        self.layout.placements.push(Placement {
            app_id: app_id.clone(),
            slot: *slot,
            x: point.x,
            y: point.y,
        });
        if self.layout.placements.len() > MAX_PLACEMENTS {
            self.layout.placements.remove(0);
        }
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        self.layout.validate()?;
        let bytes = serde_json::to_vec_pretty(&self.layout)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("layout exceeds size limit".into());
        }
        let parent = path.parent().ok_or("layout has no parent")?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let result = (|| -> std::io::Result<()> {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        Ok(())
    }
}

impl App {
    pub(super) fn register_window_placement(&mut self, index: usize) {
        if self.transient_parent_index(index).is_some() {
            return;
        }
        let window = &self.windows[index];
        let id = window.id;
        let Some(app_id) = window_app_id(&window.surface) else {
            return;
        };
        // Keep spatial bookkeeping, but opening an application is a new window,
        // not an instruction to navigate to its remembered offscreen location.
        let _ = self.layout_store.claim(id, app_id);
        self.restore_window_frame_placement(id);
    }

    pub(super) fn remember_layout(&mut self) {
        if self.layout_store.path.is_none() {
            return;
        }
        self.remember_frame_layout();
        // Only memory changes while the display is active. Disk I/O happens on
        // initialization and normal exit, never inside the render/input loop.
        if self.viewport_animation.is_none() {
            self.layout_store.layout.x = self.viewport_offset.x.clamp(-1_000_000, 1_000_000);
            self.layout_store.layout.y = self.viewport_offset.y.clamp(-1_000_000, 1_000_000);
            self.layout_store.layout.scale = self.viewport_scale;
        }
        for (index, window) in self.windows.iter().enumerate() {
            if let Some(id) = self.desktop_id(index) {
                if window.mapped && window.initial_placement_done {
                    self.layout_store.remember_desktop(&id, window.position);
                }
            } else {
                self.layout_store.remember(window.id, window.position);
            }
        }
        self.layout_store
            .claimed
            .retain(|id, _| self.windows.iter().any(|window| window.id == *id));
    }

    pub(super) fn save_layout(&mut self) {
        self.remember_layout();
        if let Err(error) = self.layout_store.save() {
            tracing::warn!(%error, "could not save spatial layout");
        }
    }
}
