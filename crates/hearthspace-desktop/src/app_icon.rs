//! Resolve application icons on the blocking worker, never during UI layout.
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use gio::prelude::SettingsExt;

use crate::{
    app_catalog::{AppCatalog, DesktopApp},
    artwork::{IconSource, RasterImage},
};

mod steam;

const ICON_SIZE: u32 = 48;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DIMENSION: u32 = 1024;

/// Resolve a window's installed artwork through the desktop icon pipeline.
/// `identity` is an app ID or WM_CLASS, never a display title.
pub fn resolve_window(catalog: &AppCatalog, identity: &str) -> Option<RasterImage> {
    if let Some(app) = catalog.app_for_window(identity) {
        return resolve_app(app);
    }
    let name = identity
        .strip_suffix(".desktop")
        .unwrap_or(identity)
        .to_lowercase();
    resolve(&IconSource {
        name: Some(name),
        ..Default::default()
    })
}

/// Resolve installed applications using both their artwork and launch identity.
pub fn resolve_app(app: &DesktopApp) -> Option<RasterImage> {
    if app.icon.as_deref() == Some("steam")
        && let Some(id) = steam::game_id(app)
        && let Some(icon) = named_icon(&format!("steam_icon_{id}"), None)
            .or_else(|| steam::cached_icon(&steam::installed_roots(), &id))
    {
        return Some(icon);
    }
    app.icon.as_deref().and_then(|name| named_icon(name, None))
}

pub fn resolve(icon: &IconSource) -> Option<RasterImage> {
    icon.name
        .as_deref()
        .and_then(|name| named_icon(name, icon.theme_path.as_deref()))
        .or_else(|| {
            icon.pixmap
                .as_ref()
                .filter(|icon| valid_pixmap(icon))
                .cloned()
        })
}

fn named_icon(name: &str, theme_path: Option<&str>) -> Option<RasterImage> {
    if name.is_empty() || name.len() > 4096 {
        return None;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return decode_file(path);
    }
    // A relative icon name is a name, not a path relative to the shell's cwd.
    if path.components().count() != 1 || matches!(name, "." | "..") {
        return None;
    }
    themed_icon(name, theme_path)
        .or_else(|| {
            // Shortcuts can reference another application's icon by its identity.
            // Sandboxed apps often export only an absolute Icon in their desktop
            // entry, without installing that name into a host icon theme.
            // Callers cache decoded artwork. Load current metadata for each new
            // fallback request so a later app installation is not hidden by a
            // second, process-lifetime catalog cache.
            catalog_icon(&AppCatalog::load(), name)
        })
        .or_else(|| {
            // Development sessions read the repository's desktop entry before the
            // icon has been installed into an XDG theme.
            (name == "hearthspace-settings")
                .then(|| decode_svg(include_bytes!("../../../assets/settings.svg")))
                .flatten()
        })
        .or_else(|| {
            // Modern Adwaita ships many standard status names only as symbolic
            // variants. Keep old AppIndicator names usable without requiring GTK.
            (!name.ends_with("-symbolic") && path.extension().is_none())
                .then(|| themed_icon(&format!("{name}-symbolic"), theme_path))
                .flatten()
        })
}

fn themed_icon(name: &str, theme_path: Option<&str>) -> Option<RasterImage> {
    if let Some(directory) = theme_path.map(Path::new).filter(|path| path.is_absolute())
        && let Some(icon) = application_icon(directory, name).and_then(|path| decode_file(&path))
    {
        return Some(icon);
    }
    // Follow the user's installed theme before using Adwaita's standard status
    // icons. Both lookups retain the library's hicolor and pixmap fallbacks.
    lookup_in_theme(name, configured_theme()).or_else(|| lookup_in_theme(name, "Adwaita"))
}

fn lookup_in_theme(name: &str, theme: &str) -> Option<RasterImage> {
    freedesktop_icons::lookup(name)
        .with_size(ICON_SIZE as u16)
        .with_theme(theme)
        .find()
        .and_then(|path| decode_file(&path))
}

fn configured_theme() -> &'static str {
    static THEME: OnceLock<String> = OnceLock::new();
    THEME.get_or_init(|| {
        let schema = gio::SettingsSchemaSource::default()
            .and_then(|source| source.lookup("org.gnome.desktop.interface", true))
            .filter(|schema| schema.has_key("icon-theme"));
        schema
            .map(|schema| {
                gio::Settings::new_full(&schema, gio::SettingsBackend::NONE, None)
                    .string("icon-theme")
                    .to_string()
            })
            .filter(|theme| !theme.is_empty())
            .unwrap_or_else(|| "Adwaita".into())
    })
}

fn catalog_icon(catalog: &AppCatalog, name: &str) -> Option<RasterImage> {
    let alias = catalog.app_for_window(name)?.icon.as_deref()?;
    if alias == name {
        return None;
    }
    if Path::new(alias).is_absolute() {
        decode_file(Path::new(alias))
    } else if Path::new(alias).components().count() == 1 {
        // One hop is sufficient; do not recursively follow cyclic icon aliases.
        themed_icon(alias, None)
    } else {
        None
    }
}

fn application_icon(directory: &Path, name: &str) -> Option<PathBuf> {
    let names = if Path::new(name).extension().is_some() {
        vec![name.to_owned()]
    } else {
        vec![format!("{name}.png"), format!("{name}.svg")]
    };
    for name in &names {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // AppIndicator IconThemePath can be a flat directory or a small private
    // icon theme. Search the latter without walking arbitrary symlink trees.
    let mut directories = vec![(directory.to_owned(), 0)];
    let mut visited = 0;
    while let Some((directory, depth)) = directories.pop() {
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > 4096 {
                return None;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if names.iter().any(|name| entry.file_name() == name.as_str()) && entry.path().is_file()
            {
                return Some(entry.path());
            }
            if kind.is_dir() && depth < 4 {
                directories.push((entry.path(), depth + 1));
            }
        }
    }
    None
}

fn decode_file(path: &Path) -> Option<RasterImage> {
    decode_file_with_shape(path, false)
}

fn decode_file_with_shape(path: &Path, require_square: bool) -> Option<RasterImage> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("svg") if !require_square => decode_svg(&bytes),
        Some("png") => decode_raster(&bytes, image::ImageFormat::Png, require_square),
        Some("jpg" | "jpeg") => decode_raster(&bytes, image::ImageFormat::Jpeg, require_square),
        _ => None,
    }
}

fn decode_raster(
    bytes: &[u8],
    format: image::ImageFormat,
    require_square: bool,
) -> Option<RasterImage> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    // Validate source dimensions before thumbnail rounding can hide a
    // near-square capsule. General application icons retain their aspect ratio.
    if require_square && decoded.width() != decoded.height() {
        return None;
    }
    let decoded = decoded.thumbnail(ICON_SIZE, ICON_SIZE).to_rgba8();
    Some(RasterImage {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
    })
}

fn decode_svg(bytes: &[u8]) -> Option<RasterImage> {
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            // Icons must be self-contained vector images. Do not expand image
            // references into additional unbounded file reads or decoders.
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let svg = std::str::from_utf8(bytes).ok()?;
    let tree = resvg::usvg::Tree::from_str(svg, &options).ok()?;
    let size = tree.size();
    let scale = ICON_SIZE as f32 / size.width().max(size.height());
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    Some(RasterImage {
        width,
        height,
        rgba,
    })
}

fn valid_pixmap(icon: &RasterImage) -> bool {
    icon.width > 0
        && icon.height > 0
        && icon.width <= MAX_DIMENSION
        && icon.height <= MAX_DIMENSION
        && u64::from(icon.width) * u64::from(icon.height) * 4 == icon.rgba.len() as u64
}

#[cfg(test)]
mod tests;
