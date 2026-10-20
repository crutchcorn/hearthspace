//! Resolve application icons on the blocking worker, never during UI layout.
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

use crate::tray::{TrayIcon, TrayPixmap};

const ICON_SIZE: u32 = 48;
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_DIMENSION: u32 = 1024;

pub fn resolve(icon: &TrayIcon) -> Option<TrayPixmap> {
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

fn named_icon(name: &str, theme_path: Option<&str>) -> Option<TrayPixmap> {
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

fn themed_icon(name: &str, theme_path: Option<&str>) -> Option<TrayPixmap> {
    if let Some(directory) = theme_path.map(Path::new).filter(|path| path.is_absolute())
        && let Some(icon) = application_icon(directory, name).and_then(|path| decode_file(&path))
    {
        return Some(icon);
    }
    // Adwaita supplies standard symbolic status icons; the lookup library also
    // follows theme inheritance and falls back to hicolor and XDG pixmaps.
    freedesktop_icons::lookup(name)
        .with_size(ICON_SIZE as u16)
        .with_theme("Adwaita")
        .find()
        .and_then(|path| decode_file(&path))
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

fn decode_file(path: &Path) -> Option<TrayPixmap> {
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
        Some("svg") => decode_svg(&bytes),
        Some("png") => decode_png(&bytes),
        _ => None,
    }
}

fn decode_png(bytes: &[u8]) -> Option<TrayPixmap> {
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(8 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .ok()?
        .thumbnail(ICON_SIZE, ICON_SIZE)
        .to_rgba8();
    Some(TrayPixmap {
        width: decoded.width(),
        height: decoded.height(),
        rgba: decoded.into_raw(),
    })
}

fn decode_svg(bytes: &[u8]) -> Option<TrayPixmap> {
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
    Some(TrayPixmap {
        width,
        height,
        rgba,
    })
}

fn valid_pixmap(icon: &TrayPixmap) -> bool {
    icon.width > 0
        && icon.height > 0
        && icon.width <= MAX_DIMENSION
        && icon.height <= MAX_DIMENSION
        && u64::from(icon.width) * u64::from(icon.height) * 4 == icon.rgba.len() as u64
}

#[cfg(test)]
mod tests;
