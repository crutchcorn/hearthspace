//! Only the user's wallpaper is sampled. Desktop windows never enter this client.
use crate::config::Config;
use gtk::{gdk, glib, prelude::*};
use hearthspace_ui_components::{Color, Palette};
use std::{io::BufReader, sync::mpsc, time::Duration};

#[path = "background/photo.rs"]
mod photo;

pub(crate) fn widget(config: &Config, monitor: &gdk::Monitor) -> gtk::Overlay {
    let palette = Palette::new(config.settings.appearance);
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    let dots = gtk::DrawingArea::new();
    dots.set_draw_func(move |_, cr, width, height| {
        let (width, height) = (f64::from(width), f64::from(height));
        color(cr, palette.app_background);
        let _ = cr.paint();
        color(cr, palette.field_dot);
        let (offset_x, offset_y) = ((width / 2.0) % 28.0, (height / 2.0) % 28.0);
        for y in 0..=(height / 28.0) as i32 {
            for x in 0..=(width / 28.0) as i32 {
                cr.arc(
                    offset_x + f64::from(x) * 28.0,
                    offset_y + f64::from(y) * 28.0,
                    1.0,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.fill();
            }
        }
        let _ = cr.save();
        cr.scale(width.max(1.0), height.max(1.0));
        let gradient = gtk::cairo::RadialGradient::new(0.5, 0.42, 0.0, 0.5, 0.42, 0.70);
        let base = palette.accent_base.to_rgba8();
        let field = palette.app_background.to_rgba8();
        let channel = |a: u8, b: u8| (f64::from(a) * 0.09 + f64::from(b) * 0.91) / 255.0;
        gradient.add_color_stop_rgba(
            0.0,
            channel(base.r, field.r),
            channel(base.g, field.g),
            channel(base.b, field.b),
            1.0,
        );
        gradient.add_color_stop_rgba(
            1.0,
            channel(base.r, field.r),
            channel(base.g, field.g),
            channel(base.b, field.b),
            0.0,
        );
        let _ = cr.set_source(&gradient);
        let _ = cr.paint();
        let _ = cr.restore();
    });
    overlay.set_child(Some(&dots));
    if let Some(path) = config.settings.background_image_path.clone() {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Fill);
        picture.set_can_shrink(true);
        overlay.add_overlay(&picture);
        let fit = config.settings.background_mode;
        let geometry = monitor.geometry();
        let (width, height) = (
            geometry.width().max(1) as u32,
            geometry.height().max(1) as u32,
        );
        let (send, receive) = mpsc::sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("lock-wallpaper".into())
            .spawn(move || {
                let result =
                    load(&path).map(|source| photo::compose(&source, fit, width, height, palette));
                let _ = send.send(result);
            });
        let weak = picture.downgrade();
        glib::timeout_add_local(Duration::from_millis(25), move || {
            let Some(picture) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            match receive.try_recv() {
                Ok(Some(image)) => {
                    let (width, height) = image.dimensions();
                    let bytes = glib::Bytes::from_owned(image.into_raw());
                    let texture = gdk::MemoryTexture::new(
                        width as i32,
                        height as i32,
                        gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        width as usize * 4,
                    );
                    picture.set_paintable(Some(&texture));
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                _ => glib::ControlFlow::Break,
            }
        });
    }
    overlay
}

fn load(path: &str) -> Option<image::RgbaImage> {
    use image::ImageDecoder;
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > 64 * 1024 * 1024 {
        return None;
    }
    let mut reader = image::ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    // Limit the final RGBA allocation as well as the decoder's source format.
    if u64::from(width) * u64::from(height) > 16 * 1024 * 1024 {
        return None;
    }
    Some(
        image::DynamicImage::from_decoder(decoder)
            .ok()?
            .into_rgba8(),
    )
}

fn color(cr: &gtk::cairo::Context, color: Color) {
    let c = color.to_rgba8();
    cr.set_source_rgba(
        f64::from(c.r) / 255.0,
        f64::from(c.g) / 255.0,
        f64::from(c.b) / 255.0,
        f64::from(c.a) / 255.0,
    );
}
