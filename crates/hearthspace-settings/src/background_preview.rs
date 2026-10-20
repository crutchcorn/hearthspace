//! A bounded thumbnail cache keeps background decoding out of repeated rebuilds.
use std::{
    fs::{self, File},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::SystemTime,
};

use hearthspace_ui_components as components;
use xilem::{
    Color, WidgetView,
    masonry::{
        imaging::Painter,
        kurbo::{Affine, Circle, Rect, Stroke},
        layout::AsUnit,
        peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    },
    view::{canvas, sized_box},
};

use crate::state::SettingsApp;

const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16_384;

#[derive(PartialEq)]
struct Key {
    path: PathBuf,
    modified: Option<SystemTime>,
    length: u64,
}

struct Preview {
    key: Key,
    image: Option<ImageBrush>,
}

fn thumbnail(path: &Path) -> Option<ImageBrush> {
    static CACHE: OnceLock<Mutex<Option<Preview>>> = OnceLock::new();
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
        return None;
    }
    let key = Key {
        path: path.to_owned(),
        modified: metadata.modified().ok(),
        length: metadata.len(),
    };
    let mut cache = CACHE.get_or_init(Mutex::default).lock().ok()?;
    if let Some(preview) = cache.as_ref()
        && preview.key == key
    {
        return preview.image.clone();
    }
    let image = decode(path);
    *cache = Some(Preview {
        key,
        image: image.clone(),
    });
    image
}

fn decode(path: &Path) -> Option<ImageBrush> {
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .resize_to_fill(168, 128, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Some(
        ImageData {
            data: image.into_raw().into(),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 168,
            height: 128,
        }
        .into(),
    )
}

pub(super) fn view(path: Option<&str>) -> impl WidgetView<SettingsApp> + use<> {
    let image = path.and_then(|path| thumbnail(Path::new(path)));
    sized_box(
        canvas(move |_: &mut SettingsApp, _, scene, size| {
            let mut painter = Painter::new(scene);
            let shape = Rect::from_origin_size((0.5, 0.5), (size.width - 1.0, size.height - 1.0))
                .to_rounded_rect(components::RADIUS_CONTROL);
            painter.fill(shape, components::APP_BACKGROUND).draw();
            painter.push_fill_clip(shape);
            if let Some(image) = image.as_ref() {
                painter.draw_image(
                    image,
                    Affine::scale_non_uniform(size.width / 168.0, size.height / 128.0),
                );
            } else {
                for x in (6..size.width as i32).step_by(12) {
                    for y in (6..size.height as i32).step_by(12) {
                        painter
                            .fill(
                                Circle::new((f64::from(x), f64::from(y)), 0.8),
                                Color::from_rgba8(0x58, 0x6f, 0x68, 38),
                            )
                            .draw();
                    }
                }
            }
            painter.pop_clip();
            painter
                .stroke(shape, &Stroke::new(1.0), components::BORDER)
                .draw();
        })
        .alt_text(""),
    )
    .fixed_width(84.0.px())
    .fixed_height(64.0.px())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_uses_selected_png_and_reloads_replaced_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("background.png");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([255, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let first = thumbnail(&path).unwrap();
        assert_eq!(first.image.width, 168);
        assert_eq!(first.image.height, 128);
        assert_eq!(first.image.data.as_ref()[..4], [255, 0, 0, 255]);
        image::RgbaImage::from_pixel(12, 16, image::Rgba([0, 0, 255, 255]))
            .save(&path)
            .unwrap();
        let second = thumbnail(&path).unwrap();
        assert_eq!(second.image.data.as_ref()[..4], [0, 0, 255, 255]);
    }

    #[test]
    fn unreadable_invalid_and_oversized_files_fall_back_without_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("background.png");
        assert!(thumbnail(&path).is_none());
        fs::write(&path, b"not a PNG").unwrap();
        assert!(thumbnail(&path).is_none());
        File::create(&path)
            .unwrap()
            .set_len(MAX_FILE_BYTES + 1)
            .unwrap();
        assert!(thumbnail(&path).is_none());
    }
}
