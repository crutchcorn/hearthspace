//! Real GLES compilation, damage tracking, and framebuffer readback.
use super::*;
use smithay::backend::{
    allocator::Fourcc,
    egl::{EGLContext, EGLDisplay, native::EGLSurfacelessDisplay},
    renderer::{
        Bind, Color32F, Offscreen,
        damage::OutputDamageTracker,
        element::{
            Element,
            utils::{Relocate, RelocateRenderElement},
        },
        gles::GlesRenderbuffer,
    },
};
use smithay::utils::Transform;

fn renderer() -> GlesRenderer {
    let display = unsafe { EGLDisplay::new(EGLSurfacelessDisplay).unwrap() };
    let context = EGLContext::new(&display).unwrap();
    unsafe { GlesRenderer::new(context).unwrap() }
}

fn pixels(
    renderer: &mut GlesRenderer,
    element: PixelShaderElement,
    origin: (i32, i32),
    density: i32,
) -> Vec<u8> {
    let size =
        smithay::utils::Size::<i32, smithay::utils::Physical>::from((64 * density, 64 * density));
    let mut buffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (size.w, size.h).into())
        .unwrap();
    let mut framebuffer = renderer.bind(&mut buffer).unwrap();
    let element =
        RelocateRenderElement::from_element(element, (-origin.0, -origin.1), Relocate::Relative);
    let mut damage = OutputDamageTracker::new(size, 1.0, Transform::Normal);
    damage
        .render_output(
            renderer,
            &mut framebuffer,
            0,
            &[element],
            Color32F::TRANSPARENT,
        )
        .unwrap();
    crate::compositor::capture::read_pixels(renderer, &framebuffer, size, false).unwrap()
}

fn pixel(bytes: &[u8], width: usize, x: usize, y: usize) -> [u8; 4] {
    bytes[(y * width + x) * 4..(y * width + x + 1) * 4]
        .try_into()
        .unwrap()
}

fn view(origin: (i32, i32)) -> Rectangle<i32, Logical> {
    Rectangle::new(origin.into(), (64, 64).into())
}

#[test]
fn glsl_dots_keep_theme_pixels_and_logical_spacing_at_both_densities() {
    let mut renderer = renderer();
    for (mode, background, dot) in [
        (ColorMode::Light, [229, 233, 227, 255], [208, 215, 209, 255]),
        (ColorMode::Dark, [20, 34, 37, 255], [41, 57, 58, 255]),
    ] {
        for density in [1, 2] {
            let mut shader = BackgroundShader::default();
            let element = shader
                .element(
                    &mut renderer,
                    None,
                    ShaderView {
                        output: view((0, 0)),
                        density,
                        camera: CanvasPoint { x: 0, y: 0 },
                        zoom: 1.,
                        mode,
                    },
                )
                .unwrap();
            assert!(shader.error.is_none());
            let image = pixels(&mut renderer, element, (0, 0), density);
            assert_eq!(pixel(&image, (64 * density) as usize, 0, 0), background);
            assert_eq!(
                pixel(
                    &image,
                    (64 * density) as usize,
                    (14 * density) as usize,
                    (14 * density) as usize
                ),
                dot
            );
            assert_eq!(
                pixel(
                    &image,
                    (64 * density) as usize,
                    (42 * density) as usize,
                    (42 * density) as usize
                ),
                dot
            );
        }
    }
}

#[test]
fn dots_remain_canvas_anchored_on_offset_outputs_and_large_negative_pans() {
    let mut renderer = renderer();
    let mut shader = BackgroundShader::default();
    let origin = (-1280, 120);
    let camera = CanvasPoint {
        x: -1_000_000_000,
        y: 1_000_000_000,
    };
    let element = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view(origin),
                density: 2,
                camera,
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    let image = pixels(&mut renderer, element, origin, 2);
    let x = (14i64 - i64::from(camera.x) - i64::from(origin.0)).rem_euclid(28) as usize * 2;
    let y = (14i64 - i64::from(camera.y) - i64::from(origin.1)).rem_euclid(28) as usize * 2;
    assert_eq!(pixel(&image, 128, x, y), [208, 215, 209, 255]);
    let moved = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view(origin),
                density: 2,
                camera: CanvasPoint {
                    x: camera.x + 1,
                    y: camera.y,
                },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert_ne!(
        image,
        pixels(&mut renderer, moved, origin, 2),
        "a single-pixel pan must survive f32 camera rounding far from origin"
    );
    let zoomed = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 2.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert_eq!(
        pixel(&pixels(&mut renderer, zoomed, (0, 0), 1), 64, 28, 28),
        [208, 215, 209, 255]
    );
}

#[test]
fn a_static_shader_reuses_identity_and_commit_until_its_view_or_theme_changes() {
    let mut renderer = renderer();
    let mut shader = BackgroundShader::default();
    let first = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    let second = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert_eq!(first.id(), second.id());
    assert_eq!(first.current_commit(), second.current_commit());
    let changed = shader
        .element(
            &mut renderer,
            None,
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 1, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert_eq!(first.id(), changed.id());
    assert_ne!(first.current_commit(), changed.current_commit());
    assert!(!shader.animated);
    let mut buffer: GlesRenderbuffer = renderer
        .create_buffer(Fourcc::Abgr8888, (64, 64).into())
        .unwrap();
    let mut framebuffer = renderer.bind(&mut buffer).unwrap();
    let mut damage = OutputDamageTracker::new((64, 64), 1.0, Transform::Normal);
    assert!(
        damage
            .render_output(
                &mut renderer,
                &mut framebuffer,
                0,
                &[first],
                Color32F::TRANSPARENT
            )
            .unwrap()
            .damage
            .is_some()
    );
    assert!(
        damage
            .render_output(
                &mut renderer,
                &mut framebuffer,
                1,
                &[second],
                Color32F::TRANSPARENT
            )
            .unwrap()
            .damage
            .is_none(),
        "an unchanged shader must not manufacture scene damage"
    );
    assert!(
        damage
            .render_output(
                &mut renderer,
                &mut framebuffer,
                1,
                &[changed],
                Color32F::TRANSPARENT
            )
            .unwrap()
            .damage
            .is_some()
    );
}

#[test]
fn invalid_glsl_falls_back_to_dots_then_a_reload_recovers_and_enforces_opacity() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("wallpaper.glsl");
    std::fs::write(&file, "not valid GLSL").unwrap();
    let mut renderer = renderer();
    let mut shader = BackgroundShader::default();
    let element = shader
        .element(
            &mut renderer,
            Some(file.to_str().unwrap()),
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert!(shader.error.is_some());
    assert_eq!(
        pixel(&pixels(&mut renderer, element, (0, 0), 1), 64, 14, 14),
        [208, 215, 209, 255]
    );
    std::fs::write(
        &file,
        "precision mediump float; void main(){gl_FragColor=vec4(1.,0.,0.,0.);}",
    )
    .unwrap();
    shader.reset();
    let element = shader
        .element(
            &mut renderer,
            Some(file.to_str().unwrap()),
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert!(shader.error.is_none());
    assert_eq!(
        pixel(&pixels(&mut renderer, element, (0, 0), 1), 64, 0, 0),
        [255, 0, 0, 255]
    );
}

#[test]
fn time_uniform_changes_real_shader_pixels_and_context_replacement_recompiles() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("animated.glsl");
    std::fs::write(&file,"precision highp float; uniform float u_time; void main(){gl_FragColor=vec4(mod(u_time,2.)*.5,0.,0.,1.);}").unwrap();
    let mut renderer_a = renderer();
    let mut shader = BackgroundShader::default();
    let first = shader
        .element(
            &mut renderer_a,
            Some(file.to_str().unwrap()),
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    let first_pixels = pixels(&mut renderer_a, first, (0, 0), 1);
    assert!(shader.animated);
    shader.started -= Duration::from_secs(1);
    let next = shader
        .element(
            &mut renderer_a,
            Some(file.to_str().unwrap()),
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert_ne!(first_pixels, pixels(&mut renderer_a, next, (0, 0), 1));
    let mut renderer_b = renderer();
    let replacement = shader
        .element(
            &mut renderer_b,
            Some(file.to_str().unwrap()),
            ShaderView {
                output: view((0, 0)),
                density: 1,
                camera: CanvasPoint { x: 0, y: 0 },
                zoom: 1.,
                mode: ColorMode::Light,
            },
        )
        .unwrap();
    assert!(shader.error.is_none());
    assert_eq!(
        pixels(&mut renderer_b, replacement, (0, 0), 1).len(),
        64 * 64 * 4
    );
}

#[path = "background_shader_edge_tests.rs"]
mod edge;
