//! Adapt tray foreground ink without changing application artwork elsewhere.
use crate::tray::TrayPixmap;
use hearthspace_ui_components::Color;

pub(super) fn themed(source: &TrayPixmap, name: Option<&str>, ink: Color) -> TrayPixmap {
    let mut image = source.clone();
    let pixels = source.rgba.as_chunks::<4>().0;
    // An opaque tile has its own background; flattening it would erase its logo.
    if !pixels.iter().any(|p| p[3] == 0) {
        return image;
    }
    let symbolic = name.is_some_and(|name| {
        let file = std::path::Path::new(name)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(name);
        let stem = file
            .strip_suffix(".svg")
            .or_else(|| file.strip_suffix(".png"))
            .unwrap_or(file);
        stem.ends_with("-symbolic") || stem.ends_with("_mono") || stem.ends_with("-mono")
    });
    let neutral = |p: &[u8; 4]| p[..3].iter().max().unwrap() - p[..3].iter().min().unwrap() <= 8;
    let visible = pixels.iter().filter(|p| p[3] >= 16).collect::<Vec<_>>();
    // Unnamed SNI pixmaps carry no symbolic flag. Only recognize a single
    // neutral ink, not arbitrary grayscale photographs or multicolor logos.
    let single_ink = !visible.is_empty()
        && visible.iter().all(|p| neutral(p))
        && visible.iter().map(|p| p[0]).max().unwrap()
            - visible.iter().map(|p| p[0]).min().unwrap()
            <= 32;
    if !symbolic && !single_ink {
        return image;
    }
    let [r, g, b, _] = ink.to_rgba8().to_u8_array();
    for pixel in image.rgba.as_chunks_mut::<4>().0 {
        if pixel[3] != 0 && neutral(pixel) {
            // Keep coverage/antialiasing and colored status badges intact.
            pixel[..3].copy_from_slice(&[r, g, b]);
        }
    }
    image
}

#[cfg(test)]
mod tests;
