use super::*;

#[test]
fn time_in_comments_or_an_unused_declaration_does_not_schedule_animation() {
    assert!(!uses_time("// u_time u_time\n void main(){}"));
    assert!(!uses_time("/* u_time */ uniform float u_time;"));
    assert!(uses_time(
        "uniform float u_time; void main(){float t=u_time;}"
    ));
}

#[test]
fn discarded_custom_fragments_leave_the_clear_instead_of_claiming_opaque_damage() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("discard.glsl");
    std::fs::write(&file, "precision mediump float; void main(){discard;}").unwrap();
    let mut renderer = renderer();
    let mut shader = BackgroundShader::default();
    let element = shader
        .element(
            &mut renderer,
            file.to_str(),
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
    assert!(element.opaque_regions(1.0.into()).is_empty());
    // The helper clears transparent; the desktop clears opaque black. No old
    // framebuffer contents may survive merely because the wallpaper discarded.
    assert_eq!(
        pixel(&pixels(&mut renderer, element, (0, 0), 1), 64, 0, 0),
        [0, 0, 0, 0]
    );
}
