use super::*;

const RED_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="8"><rect width="16" height="8" fill="#ff0000" fill-opacity="0.5"/></svg>"##;

fn fallback() -> RasterImage {
    RasterImage {
        width: 1,
        height: 1,
        rgba: vec![0, 255, 0, 255],
    }
}

#[test]
fn settings_desktop_identity_resolves_to_a_transparent_app_icon() {
    let desktop = include_str!("../../../../data/applications/dev.hearthspace.Settings.desktop");
    let name = desktop
        .lines()
        .find_map(|line| line.strip_prefix("Icon="))
        .unwrap();
    let rendered = resolve(&IconSource {
        name: Some(name.into()),
        ..Default::default()
    })
    .expect("the Settings launcher and dock icon is available before installation");
    assert_eq!((rendered.width, rendered.height), (48, 48));
    assert!(
        rendered
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] == 0)
    );
    assert!(
        rendered
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0)
    );
}

#[test]
fn private_theme_png_and_svg_override_fallback_without_losing_alpha() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("example.svg"), RED_SVG).unwrap();
    let mut icon = IconSource {
        name: Some("example".into()),
        theme_path: Some(directory.path().to_string_lossy().into_owned()),
        pixmap: Some(fallback()),
    };
    let rendered = resolve(&icon).unwrap();
    assert_eq!((rendered.width, rendered.height), (48, 24));
    assert_eq!(&rendered.rgba[..4], &[255, 0, 0, 128]);
    let png = image::RgbaImage::from_pixel(2, 2, image::Rgba([20, 30, 40, 200]));
    png.save(directory.path().join("example.png")).unwrap();
    icon.name = Some("example.png".into());
    let rendered = resolve(&icon).unwrap();
    assert_eq!(&rendered.rgba[..4], &[20, 30, 40, 200]);
}

#[test]
fn nested_theme_and_absolute_paths_are_supported() {
    let directory = tempfile::tempdir().unwrap();
    let nested = directory.path().join("hicolor/scalable/status");
    fs::create_dir_all(&nested).unwrap();
    let path = nested.join("nested.svg");
    fs::write(&path, RED_SVG).unwrap();
    assert_eq!(
        application_icon(directory.path(), "nested"),
        Some(path.clone())
    );
    let icon = IconSource {
        name: Some(path.to_string_lossy().into_owned()),
        ..Default::default()
    };
    assert_eq!(resolve(&icon).unwrap().width, 48);
}

#[test]
fn standard_status_name_uses_available_symbolic_variant() {
    let directory = tempfile::tempdir().unwrap();
    let name = "hearthspace-test-status-unique";
    fs::write(
        directory.path().join(format!("{name}-symbolic.svg")),
        RED_SVG,
    )
    .unwrap();
    let icon = IconSource {
        name: Some(name.into()),
        theme_path: Some(directory.path().to_string_lossy().into_owned()),
        pixmap: Some(fallback()),
    };
    let rendered = resolve(&icon).unwrap();
    assert_eq!((rendered.width, rendered.height), (48, 24));
    assert_eq!(&rendered.rgba[..4], &[255, 0, 0, 128]);
}

#[test]
fn missing_malformed_and_oversized_icons_use_validated_pixmap_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bad.png");
    fs::write(&path, b"not a png").unwrap();
    let mut icon = IconSource {
        name: Some(path.to_string_lossy().into_owned()),
        pixmap: Some(fallback()),
        ..Default::default()
    };
    assert_eq!(resolve(&icon), Some(fallback()));
    let file = File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    assert_eq!(resolve(&icon), Some(fallback()));
    icon.name = Some("../bad.png".into());
    assert_eq!(resolve(&icon), Some(fallback()));
    icon.pixmap.as_mut().unwrap().width = u32::MAX;
    assert_eq!(resolve(&icon), None);
    icon.pixmap = Some(RasterImage {
        width: 2,
        ..fallback()
    });
    assert_eq!(resolve(&icon), None);
}

#[test]
fn png_dimension_limit_and_svg_external_resources_are_enforced() {
    let png = image::RgbaImage::new(MAX_DIMENSION + 1, 1);
    let mut bytes = Cursor::new(Vec::new());
    png.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
    assert!(decode_raster(bytes.get_ref(), image::ImageFormat::Png, false).is_none());
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><image href="file:///tmp/absent.svg" width="16" height="16"/></svg>"#;
    assert!(decode_svg(svg).unwrap().rgba.iter().all(|byte| *byte == 0));
}
