use super::*;
use crate::compositor::windows::titlebar_button_rect;

fn state() -> TitlebarState {
    TitlebarState {
        title: "Hearthspace Settings".into(),
        app_id: Some("dev.hearthspace.Settings".into()),
        active: true,
        maximized: false,
        hovered: None,
        pressed: None,
        photo: false,
    }
}

#[test]
fn foreground_leaves_material_grip_divider_and_corner_coverage_to_the_gpu() {
    let image = rasterize_titlebar(678, &state(), 1);
    assert_eq!(image.dimensions(), (678, 44));
    for (x, y) in [(0, 0), (677, 0), (400, 10), (339, 22), (0, 43)] {
        assert_eq!(image.get_pixel(x, y).0[3], 0);
    }
    assert!(
        image
            .pixels()
            .all(|pixel| pixel.0[..3].iter().all(|channel| *channel <= pixel.0[3]))
    );
}

#[test]
fn active_state_changes_foreground_without_baking_an_outer_rim() {
    let mut state = state();
    let active = rasterize_titlebar(678, &state, 1);
    state.active = false;
    let inactive = rasterize_titlebar(678, &state, 1);
    assert_eq!(active.get_pixel(339, 0), inactive.get_pixel(339, 0));
    assert!(region(&active, 13, 14, 15, 15) != region(&inactive, 13, 14, 15, 15));
}

#[test]
fn close_hover_and_held_feedback_stay_inside_the_shared_action_geometry() {
    let mut state = state();
    let idle = rasterize_titlebar(678, &state, 1);
    state.hovered = Some(TitlebarButton::Close);
    let hovered = rasterize_titlebar(678, &state, 1);
    state.pressed = Some(TitlebarButton::Close);
    let pressed = rasterize_titlebar(678, &state, 1);
    assert!(hovered == pressed, "reference has no separate pressed fill");
    let close = titlebar_button_rect(678, TitlebarButton::Close);
    let [r, g, b, a] = hovered.get_pixel((close.loc.x + 5) as u32, 22).0;
    assert!(
        u16::from(r) > u16::from(g) * 2 && g > b && a == 255,
        "close hover must be red"
    );
    assert!(
        region(&idle, close.loc.x as u32, 6, 32, 31)
            != region(&hovered, close.loc.x as u32, 6, 32, 31)
    );
    assert_eq!(idle.get_pixel(339, 22), hovered.get_pixel(339, 22));
}

#[test]
fn maximize_restore_and_application_identity_render_distinct_shared_glyphs() {
    let mut state = state();
    let normal = rasterize_titlebar(678, &state, 1);
    state.maximized = true;
    let maximized = rasterize_titlebar(678, &state, 1);
    let rect = titlebar_button_rect(678, TitlebarButton::Maximize);
    assert!(
        region(&normal, rect.loc.x as u32, 6, 32, 31)
            != region(&maximized, rect.loc.x as u32, 6, 32, 31)
    );
    state.app_id = Some("foot".into());
    let terminal = rasterize_titlebar(678, &state, 1);
    assert!(region(&normal, 13, 14, 15, 15) != region(&terminal, 13, 14, 15, 15));
}

#[test]
fn named_system_font_renders_visible_title_text_at_native_scale() {
    let mut state = state();
    let named = rasterize_titlebar(678, &state, 1);
    state.title.clear();
    let empty = rasterize_titlebar(678, &state, 1);
    assert!(region(&named, 44, 6, 250, 32) != region(&empty, 44, 6, 250, 32));
}

#[test]
fn long_titles_leave_the_center_grip_and_controls_clear_at_high_scale() {
    let mut state = state();
    state.title.clear();
    let empty = rasterize_titlebar(240, &state, 2);
    state.title = "A very long terminal title which must not cover its controls".into();
    let long = rasterize_titlebar(240, &state, 2);
    assert_eq!(long.dimensions(), (480, 88));
    assert!(
        region(&long, 400, 28, 72, 32)
            .iter()
            .any(|pixel| pixel.0[3] > 0),
        "control glyphs must paint at physical 2× positions"
    );
    assert!(
        region(&empty, 196, 0, 284, 88) == region(&long, 196, 0, 284, 88),
        "a long title must not change center grip or action pixels"
    );
    assert!(
        region(&empty, 88, 20, 84, 44) != region(&long, 88, 20, 84, 44),
        "title text must remain visible within its allocated region at 2×"
    );
}

#[test]
fn resizing_reuses_every_bitmap_and_keeps_controls_at_the_hit_rectangles() {
    let state = state();
    let mut cache = TitlebarBuffer::new(state.clone(), 2);
    let original = [&cache.title, &cache.ellipsis, &cache.app, &cache.actions].map(Arc::clone);
    for width in [240, 678, 1024, 320, 679] {
        cache.update(state.clone(), 2);
        for (before, after) in
            original
                .iter()
                .zip([&cache.title, &cache.ellipsis, &cache.app, &cache.actions])
        {
            assert!(
                Arc::ptr_eq(before, after),
                "resize must retain the actual bitmap and Smithay buffer identity"
            );
        }
        let parts = cache.placements(f64::from(width));
        let actions = parts.last().unwrap();
        for button in [TitlebarButton::Maximize, TitlebarButton::Close] {
            assert_eq!(
                actions.x + f64::from(titlebar_button_rect(ACTIONS_WIDTH, button).loc.x),
                f64::from(titlebar_button_rect(width, button).loc.x)
            );
        }
        let ellipsis_visible = parts
            .iter()
            .any(|part| std::ptr::eq(part.raster, cache.ellipsis.as_ref()));
        assert_eq!(
            ellipsis_visible,
            cache.title.natural_width > (f64::from(width) / 2.0 - 78.0)
        );
    }
}

#[test]
fn hovering_only_rasterizes_the_small_actions_strip() {
    let mut state = state();
    let mut cache = TitlebarBuffer::new(state.clone(), 1);
    let title = Arc::clone(&cache.title);
    let ellipsis = Arc::clone(&cache.ellipsis);
    let app = Arc::clone(&cache.app);
    let actions = Arc::clone(&cache.actions);
    state.hovered = Some(TitlebarButton::Close);
    cache.update(state, 1);
    assert!(Arc::ptr_eq(&title, &cache.title));
    assert!(Arc::ptr_eq(&ellipsis, &cache.ellipsis));
    assert!(Arc::ptr_eq(&app, &cache.app));
    assert!(!Arc::ptr_eq(&actions, &cache.actions));
    assert_eq!(cache.actions.pixels.dimensions(), (128, 44));
}

#[test]
fn enormous_titles_have_bounded_bitmaps_and_still_show_ellipsis() {
    let mut state = state();
    state.title = "Long terminal title ".repeat(400);
    let cache = TitlebarBuffer::new(state, 1);
    assert_eq!(cache.title.pixels.width(), MAX_TITLE_WIDTH as u32);
    assert!(cache.title.natural_width > MAX_TITLE_WIDTH);
    assert!(
        cache
            .placements(6000.0)
            .iter()
            .any(|part| std::ptr::eq(part.raster, cache.ellipsis.as_ref()))
    );
}

fn rasterize_titlebar(width: i32, state: &TitlebarState, scale: i32) -> image::RgbaImage {
    let cache = TitlebarBuffer::new(state.clone(), scale);
    let mut image = image::RgbaImage::new((width * scale) as u32, (44 * scale) as u32);
    for part in cache.placements(f64::from(width)) {
        let start = (part.x * f64::from(scale)).round() as i32;
        let columns = (part.width * f64::from(scale)) as u32;
        for y in 0..image.height() {
            for x in 0..columns {
                let target = start + x as i32;
                if target >= 0 && target < image.width() as i32 {
                    let pixel = part.raster.pixels.get_pixel(x, y);
                    if pixel.0[3] > 0 {
                        image.put_pixel(target as u32, y, *pixel);
                    }
                }
            }
        }
    }
    image
}

fn region(image: &image::RgbaImage, x: u32, y: u32, w: u32, h: u32) -> Vec<image::Rgba<u8>> {
    (y..y + h)
        .flat_map(|y| (x..x + w).map(move |x| *image.get_pixel(x, y)))
        .collect()
}
