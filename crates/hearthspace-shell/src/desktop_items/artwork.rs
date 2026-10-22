//! File previews and MIME icons, used only by the desktop artwork worker.
//!
//! Reuse GIO's validated desktop thumbnails without writing to the user's cache.
//! Cold-cache FLAC files use their embedded cover; every failure keeps a MIME icon.
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
    sync::Arc,
};

use gio::prelude::*;

use crate::{
    app_icon,
    tray::{TrayIcon, TrayPixmap},
};

mod cover;
#[cfg(test)]
mod tests;

pub(crate) type Images = BTreeMap<String, Arc<TrayPixmap>>;
pub(crate) type Files = BTreeMap<String, PathBuf>;

// One poll cannot decode a directory full of media at once. Remaining items
// retain their MIME icons and are filled by subsequent polls.
const MAX_PREVIEWS_PER_POLL: usize = 8;

#[derive(Default)]
pub(crate) struct Cache {
    entries: BTreeMap<PathBuf, Entry>,
    icons: BTreeMap<String, Option<Arc<TrayPixmap>>>,
}

struct Entry {
    source: Source,
    image: Option<Arc<TrayPixmap>>,
    preview_loaded: bool,
}

#[derive(Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Stamp {
    fn read(path: &Path) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        metadata.is_file().then(|| Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Source {
    stamp: Stamp,
    thumbnail: Option<(PathBuf, Stamp)>,
    content_type: String,
    icons: Vec<String>,
}

impl Source {
    fn read(path: &Path) -> Option<Self> {
        let stamp = Stamp::read(path)?;
        let info = gio::File::for_path(path)
            .query_info(
                "standard::content-type,standard::icon,thumbnail::*",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .ok();
        let content_type = info
            .as_ref()
            .and_then(|info| info.content_type())
            .unwrap_or_else(|| gio::content_type_guess(Some(path), None).0)
            .to_string();
        let icon = info
            .as_ref()
            .and_then(|info| info.icon())
            .unwrap_or_else(|| gio::content_type_get_icon(&content_type));
        let mut icons = icon_names(&icon);
        // The type-specific name normally falls back to an audio/image/text
        // generic name through GThemedIcon. Keep one last document fallback.
        icons.push("text-x-generic".into());
        let thumbnail = info.as_ref().and_then(valid_thumbnail);
        Some(Self {
            stamp,
            thumbnail,
            content_type,
            icons,
        })
    }

    fn preview(&self, path: &Path) -> Option<TrayPixmap> {
        self.thumbnail
            .as_ref()
            // Freedesktop cache validation uses whole-second mtimes. A file
            // replaced within that second (or with a preserved mtime) must not
            // reuse a thumbnail that predates its last filesystem change.
            .filter(|(_, thumbnail)| thumbnail.modified >= self.stamp.changed)
            .and_then(|(path, _)| cover::image_file(path))
            .or_else(|| {
                matches!(self.content_type.as_str(), "audio/flac" | "audio/x-flac")
                    .then(|| cover::flac(path))
                    .flatten()
            })
    }
}

fn valid_thumbnail(info: &gio::FileInfo) -> Option<(PathBuf, Stamp)> {
    // GNOME's large thumbnails have size-specific attributes. The unsuffixed
    // pair only covers normal thumbnails on older GLib versions.
    [
        ("thumbnail::path-large", "thumbnail::is-valid-large"),
        ("thumbnail::path-normal", "thumbnail::is-valid-normal"),
        ("thumbnail::path", "thumbnail::is-valid"),
    ]
    .into_iter()
    .find_map(|(path_attribute, valid_attribute)| {
        if !info.boolean(valid_attribute) {
            return None;
        }
        let path = info.attribute_byte_string(path_attribute)?;
        let path = PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes()));
        if !path.is_absolute() {
            return None;
        }
        let stamp = Stamp::read(&path)?;
        Some((path, stamp))
    })
}

fn icon_names(icon: &gio::Icon) -> Vec<String> {
    if let Some(icon) = icon.downcast_ref::<gio::ThemedIcon>() {
        return icon
            .names()
            .into_iter()
            .map(|name| name.to_string())
            .collect();
    }
    if let Some(icon) = icon.downcast_ref::<gio::FileIcon>() {
        return icon
            .file()
            .path()
            .and_then(|path| path.to_str().map(str::to_owned))
            .into_iter()
            .collect();
    }
    Vec::new()
}

impl Cache {
    pub(crate) fn refresh(&mut self, files: &Files) -> Images {
        let paths: BTreeSet<_> = files.values().collect();
        self.entries.retain(|path, _| paths.contains(path));
        let mut remaining = MAX_PREVIEWS_PER_POLL;
        let mut images = Images::new();
        for (id, path) in files.iter().take(hearthspace_common::MAX_DESKTOP_ITEMS) {
            let Some(source) = Source::read(path) else {
                self.entries.remove(path);
                continue;
            };
            if self
                .entries
                .get(path)
                .is_none_or(|entry| entry.source != source)
            {
                let image = source.icons.iter().find_map(|name| {
                    self.icons
                        .entry(name.clone())
                        .or_insert_with(|| {
                            app_icon::resolve(&TrayIcon {
                                name: Some(name.clone()),
                                ..Default::default()
                            })
                            .map(Arc::new)
                        })
                        .clone()
                });
                self.entries.insert(
                    path.clone(),
                    Entry {
                        source,
                        image,
                        preview_loaded: false,
                    },
                );
            }
            let entry = self.entries.get_mut(path).expect("inserted file artwork");
            if !entry.preview_loaded
                && entry.source.thumbnail.is_none()
                && !matches!(
                    entry.source.content_type.as_str(),
                    "audio/flac" | "audio/x-flac"
                )
            {
                entry.preview_loaded = true;
            }
            if !entry.preview_loaded && remaining > 0 {
                remaining -= 1;
                if let Some(image) = entry.source.preview(path) {
                    entry.image = Some(Arc::new(image));
                }
                // Cache failures too: missing/corrupt covers must not repeatedly
                // reopen a large media file until the file actually changes.
                entry.preview_loaded = true;
            }
            if let Some(image) = &entry.image {
                images.insert(id.clone(), image.clone());
            }
        }
        // Bound MIME/theme-name history, including files removed since last poll.
        if self.icons.len() > 128 {
            self.icons.clear();
        }
        images
    }
}
