//! Real GPU pixels protect full-window chrome, clipping and retained damage.
use smithay::{
    backend::{
        allocator::Fourcc,
        egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
        renderer::{
            Bind, Color32F, Offscreen,
            damage::OutputDamageTracker,
            element::{
                Element, Kind, render_elements,
                solid::{SolidColorBuffer, SolidColorRenderElement},
                texture::{TextureRenderBuffer, TextureRenderElement},
                utils::{CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement},
            },
            gles::GlesRenderbuffer,
        },
    },
    utils::{Physical, Size, Transform},
};

use super::*;

render_elements! {
    TestElement<=GlesRenderer>;
    Frame = PixelShaderElement,
    Solid = SolidColorRenderElement,
    Texture = TextureRenderElement<GlesTexture>,
    ClippedSolid = RoundedElement<SolidColorRenderElement>,
    ClippedTexture = RoundedElement<TextureRenderElement<GlesTexture>>,
    CroppedTexture = CropRenderElement<RoundedElement<TextureRenderElement<GlesTexture>>>,
    Scaled = RescaleRenderElement<RelocateRenderElement<RoundedElement<SolidColorRenderElement>>>,
    ScaledFrame = RescaleRenderElement<RelocateRenderElement<PixelShaderElement>>,
}

#[path = "focused_tests.rs"]
mod focused;
#[path = "header_tests.rs"]
mod header;
#[path = "panel_tests.rs"]
mod panel;

fn renderer() -> GlesRenderer {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    unsafe { GlesRenderer::new(context).unwrap() }
}

fn capture(
    renderer: &mut GlesRenderer,
    size: Size<i32, Physical>,
    transform: Transform,
    elements: &[TestElement],
) -> Vec<u8> {
    let mut framebuffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())
        .unwrap();
    let mut target = renderer.bind(&mut framebuffer).unwrap();
    OutputDamageTracker::new(size, 1.0, transform)
        .render_output(renderer, &mut target, 0, elements, Color32F::TRANSPARENT)
        .unwrap();
    crate::compositor::capture::read_pixels(renderer, &target, size, false).unwrap()
}

fn pixel(pixels: &[u8], width: i32, x: i32, y: i32) -> &[u8] {
    let offset = ((y * width + x) * 4) as usize;
    &pixels[offset..offset + 4]
}

fn solid(buffer: &SolidColorBuffer, location: (i32, i32)) -> SolidColorRenderElement {
    SolidColorRenderElement::from_buffer(buffer, location, 1.0, 1.0, Kind::Unspecified)
}

#[test]
fn active_rim_surrounds_the_body_and_preserves_content_and_popup_stacking() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((80, 60).into(), (160, 100).into());
    let frame = cache
        .elements(&mut renderer, 1, window, 1.0, true, false, false, false)
        .unwrap();
    assert!(
        frame.clip.is_none(),
        "client decorations must retain their own corners"
    );
    let body = SolidColorBuffer::new((160, 100), [0.2, 0.2, 0.2, 1.0]);
    let popup = SolidColorBuffer::new((10, 10), [0.0, 0.0, 1.0, 1.0]);
    let image = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[
            solid(&popup, (75, 105)).into(),
            frame.rim.into(),
            solid(&body, (80, 60)).into(),
            frame.shadow.into(),
        ],
    );
    for (x, y) in [(79, 90), (240, 110), (160, 160)] {
        let rim = pixel(&image, 320, x, y);
        assert!(
            rim[0] > rim[1] + 20 && rim[3] > 120,
            "missing orange rim at {x},{y}: {rim:?}"
        );
    }
    assert_eq!(pixel(&image, 320, 160, 110), &[51, 51, 51, 255]);
    assert_eq!(pixel(&image, 320, 79, 110), &[0, 0, 255, 255]);
    assert_eq!(
        pixel(&image, 320, 80, 60)[3],
        255,
        "a CSD corner must not be forcibly rounded"
    );
}

#[test]
fn frame_damage_tracks_hover_and_focus_without_shadowing_the_window_interior() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((80, 60).into(), (160, 100).into());
    let first = cache
        .elements(&mut renderer, 1, window, 1.0, false, false, true, false)
        .unwrap();
    let again = cache
        .elements(&mut renderer, 1, window, 1.0, false, false, true, false)
        .unwrap();
    assert_eq!(first.rim.id(), again.rim.id());
    assert_eq!(first.rim.current_commit(), again.rim.current_commit());
    assert_eq!(first.shadow.current_commit(), again.shadow.current_commit());
    let hovered = cache
        .elements(&mut renderer, 1, window, 1.0, false, true, true, false)
        .unwrap();
    assert_eq!(first.shadow.id(), hovered.shadow.id());
    assert_ne!(
        first.shadow.current_commit(),
        hovered.shadow.current_commit()
    );
    let idle_image = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[first.shadow.clone().into()],
    );
    let hover_image = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[hovered.shadow.into()],
    );
    assert!(
        pixel(&hover_image, 320, 160, 180)[3] > pixel(&idle_image, 320, 160, 180)[3],
        "an inactive hovered window should gain the stronger panel shadow"
    );
    let hover_border = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[hovered.rim.into()],
    );
    assert_eq!(
        pixel(&hover_border, 320, 79, 110),
        &[112, 112, 112, 112],
        "hover must retain the white inactive border"
    );
    assert_eq!(pixel(&hover_image, 320, 82, 110), &[0, 0, 0, 0]);
    let focused = cache
        .elements(&mut renderer, 1, window, 1.0, true, false, true, false)
        .unwrap();
    assert_eq!(first.rim.id(), focused.rim.id());
    assert_ne!(first.rim.current_commit(), focused.rim.current_commit());
    let focused_hover = cache
        .elements(&mut renderer, 1, window, 1.0, true, true, true, false)
        .unwrap();
    assert_eq!(
        focused.rim.current_commit(),
        focused_hover.rim.current_commit(),
        "the active material overrides hover without unnecessary damage"
    );
    let inset = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[focused.rim.into()],
    );
    let glow = pixel(&inset, 320, 82, 110);
    assert!(
        glow[0] > glow[1] && (1..=4).contains(&glow[3]),
        "the active inset should have a faint orange glow: {glow:?}"
    );
    assert_eq!(pixel(&inset, 320, 160, 110), &[0, 0, 0, 0]);
    let image = capture(
        &mut renderer,
        (320, 240).into(),
        Transform::Normal,
        &[focused.shadow.into()],
    );
    assert_eq!(pixel(&image, 320, 160, 110), &[0, 0, 0, 0]);
    assert!(
        pixel(&image, 320, 160, 180)[3] > 5,
        "the shadow should extend below the body"
    );
    cache.retain(&HashSet::new());
    let reopened = cache
        .elements(&mut renderer, 1, window, 1.0, false, false, true, false)
        .unwrap();
    assert_ne!(first.rim.id(), reopened.rim.id());
}

#[test]
fn single_pixel_surfaces_round_all_corners_and_reveal_the_scene_below() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((60, 50).into(), (180, 130).into());
    let frame = cache
        .elements(&mut renderer, 1, window, 1.0, true, false, true, false)
        .unwrap();
    let body = SolidColorBuffer::new((180, 130), [1.0, 0.0, 0.0, 1.0]);
    let background = SolidColorBuffer::new((300, 240), [0.0, 1.0, 0.0, 1.0]);
    let clipped = frame.clip.unwrap().wrap(
        solid(&body, (60, 50)),
        Some(Color32F::new(1.0, 0.0, 0.0, 1.0)),
    );
    let image = capture(
        &mut renderer,
        (300, 240).into(),
        Transform::Normal,
        &[clipped.into(), solid(&background, (0, 0)).into()],
    );
    for (x, y) in [(60, 50), (239, 50), (60, 179), (239, 179)] {
        assert_eq!(pixel(&image, 300, x, y), &[0, 255, 0, 255]);
    }
    assert_eq!(pixel(&image, 300, 150, 110), &[255, 0, 0, 255]);
    assert_eq!(pixel(&image, 300, 61, 110), &[255, 0, 0, 255]);
}

#[test]
fn texture_clipping_survives_source_crop_and_does_not_leak_to_the_next_window() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((60, 50).into(), (180, 130).into());
    let frame = cache
        .elements(&mut renderer, 1, window, 1.0, true, false, true, false)
        .unwrap();
    let bytes: Vec<u8> = [255, 0, 0, 255].repeat(64 * 64);
    let buffer = TextureRenderBuffer::from_memory(
        &mut renderer,
        &bytes,
        Fourcc::Abgr8888,
        (64, 64),
        false,
        1,
        Transform::_90,
        Some(vec![Rectangle::from_size((64, 64).into())]),
    )
    .unwrap();
    let texture = |location, size| {
        TextureRenderElement::from_texture_render_buffer(
            location,
            &buffer,
            None,
            None,
            Some(size),
            Kind::Unspecified,
        )
    };
    let rounded = frame
        .clip
        .unwrap()
        .wrap(texture((60.0, 50.0), (180, 130).into()), None);
    // The right half is a source crop after clipping; its lower-right corner
    // must still be relative to the full window, not the cropped rectangle.
    let cropped = CropRenderElement::from_element(
        rounded,
        1.0,
        Rectangle::new((150, 50).into(), (90, 130).into()),
    )
    .unwrap();
    let image = capture(
        &mut renderer,
        (300, 240).into(),
        Transform::Normal,
        &[
            cropped.into(),
            texture((10.0, 10.0), (20, 20).into()).into(),
        ],
    );
    assert_eq!(pixel(&image, 300, 239, 179), &[0, 0, 0, 0]);
    assert_eq!(pixel(&image, 300, 160, 100), &[255, 0, 0, 255]);
    assert_eq!(
        pixel(&image, 300, 10, 10),
        &[255, 0, 0, 255],
        "the default shader must be restored after the clipped draw"
    );
}

#[test]
fn rounded_windows_follow_output_offsets_zoom_and_physical_output_scale() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    // A 100x80 client at 50% canvas zoom on a 2x output has its original
    // physical size. Its output occupies negative desktop coordinates.
    let window = Rectangle::new((-90, -40).into(), (50, 40).into());
    let frame = cache
        .elements(&mut renderer, 1, window, 0.5, true, false, true, false)
        .unwrap();
    let body = SolidColorBuffer::new((50, 40), [1.0, 0.0, 0.0, 1.0]);
    let rounded = frame.clip.unwrap().wrap(
        solid(&body, (-90, -40)),
        Some(Color32F::new(1.0, 0.0, 0.0, 1.0)),
    );
    let element = RescaleRenderElement::from_element(
        RelocateRenderElement::from_element(rounded, (100, 50), Relocate::Relative),
        (0, 0).into(),
        2.0,
    );
    let image = capture(
        &mut renderer,
        (160, 140).into(),
        Transform::Normal,
        &[element.into()],
    );
    assert_eq!(pixel(&image, 160, 20, 20), &[0, 0, 0, 0]);
    assert_eq!(pixel(&image, 160, 119, 99), &[0, 0, 0, 0]);
    assert_eq!(pixel(&image, 160, 70, 60), &[255, 0, 0, 255]);
    assert_eq!(pixel(&image, 160, 30, 30), &[255, 0, 0, 255]);
}

#[test]
fn rotated_output_preserves_the_rounded_surface_coverage() {
    let mut renderer = renderer();
    let mut cache = WindowFrameCache::default();
    let window = Rectangle::new((40, 60).into(), (160, 120).into());
    let frame = cache
        .elements(&mut renderer, 1, window, 1.0, true, false, true, false)
        .unwrap();
    let body = SolidColorBuffer::new((160, 120), [1.0, 0.0, 0.0, 1.0]);
    let clip = frame.clip.unwrap();
    let scene = || {
        vec![
            clip.wrap(
                solid(&body, (40, 60)),
                Some(Color32F::new(1.0, 0.0, 0.0, 1.0)),
            )
            .into(),
        ]
    };
    let normal = capture(
        &mut renderer,
        (256, 256).into(),
        Transform::Normal,
        &scene(),
    );
    let rotated = capture(&mut renderer, (256, 256).into(), Transform::_90, &scene());
    let opaque = |pixels: &[u8]| {
        pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] == 255)
            .count()
    };
    assert_eq!(opaque(&normal), opaque(&rotated));
    assert!(
        opaque(&normal) > 18_000 && opaque(&normal) < 19_200,
        "rounding must remove corners, while keeping the complete window body"
    );
}
