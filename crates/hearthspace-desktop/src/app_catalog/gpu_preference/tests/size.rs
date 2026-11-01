use super::*;

const LIMIT: usize = 1024 * 1024;

fn boundary_entry(with_preference: bool) -> String {
    let mut contents =
        "[Desktop Entry]\nType=Application\nName=Boundary\nExec=/bin/true\n".to_string();
    if with_preference {
        contents.push_str("PrefersNonDefaultGPU=true\n");
    }
    contents.push_str("X-Padding=");
    contents.push_str(&"x".repeat(LIMIT - contents.len() - 1));
    contents.push('\n');
    assert_eq!(contents.len(), LIMIT);
    contents
}

#[test]
fn adding_preference_cannot_publish_an_override_larger_than_the_reader_limit() {
    let contents = boundary_entry(false);
    for existing_override in [false, true] {
        let fixture = Fixture::new("vendor/studio.desktop", &contents);
        let local = fixture.user.join("applications/vendor/studio.desktop");
        if existing_override {
            fs::create_dir_all(local.parent().unwrap()).unwrap();
            fs::write(&local, &contents).unwrap();
        }
        let catalog = AppCatalog::load_from_data_dirs(fixture.dirs());
        assert_eq!(catalog.apps().len(), 1, "exact-limit input is readable");
        assert!(!catalog.apps()[0].prefers_non_default_gpu);
        assert!(
            fixture
                .save("vendor-studio.desktop", true)
                .unwrap_err()
                .contains("too large")
        );
        assert_eq!(
            fs::read_to_string(fixture.system.join("applications/vendor/studio.desktop")).unwrap(),
            contents
        );
        if existing_override {
            assert_eq!(fs::read_to_string(&local).unwrap(), contents);
        } else {
            assert!(!local.exists());
        }
        assert!(
            !fixture
                .user
                .join("applications/vendor-studio.desktop")
                .exists()
        );
        assert_eq!(
            AppCatalog::load_from_data_dirs(fixture.dirs()).apps(),
            catalog.apps()
        );
    }
}

#[test]
fn serialized_output_exactly_at_the_reader_limit_remains_readable() {
    let fixture = Fixture::new("vendor/studio.desktop", &boundary_entry(true));
    let saved = fixture.save("vendor-studio.desktop", true).unwrap();
    assert_eq!(fs::metadata(&saved.path).unwrap().len(), LIMIT as u64);
    assert!(saved.prefers_non_default_gpu);
    assert_eq!(
        AppCatalog::load_from_data_dirs(fixture.dirs()).apps(),
        &[saved]
    );
}
