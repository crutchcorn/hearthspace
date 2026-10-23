use super::*;
use crate::app_catalog::AppCatalog;

const HASH: &str = "279871be81ffe844e0ac24a1afea74af33126822";
const OTHER_HASH: &str = "0123456789abcdef0123456789abcdef01234567";

fn app(exec: &str) -> DesktopApp {
    let root = tempfile::tempdir().unwrap();
    let applications = root.path().join("applications");
    fs::create_dir_all(&applications).unwrap();
    fs::write(applications.join("game.desktop"), format!(
        "[Desktop Entry]\nType=Application\nName=steam://rungameid/999\nExec={exec}\nIcon=steam\n"
    )).unwrap();
    AppCatalog::load_from_data_dirs(vec![root.path().into()]).apps()[0].clone()
}

fn jpeg(path: &Path, width: u32, height: u32) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(width, height, image::Rgb([230, 40, 70]))
        .save(path)
        .unwrap();
}

fn cached(root: &Path) -> Option<TrayPixmap> {
    cached_icon(&[root.to_owned()], "4122860")
}

#[test]
fn quoted_steam_executables_and_field_codes_use_the_launch_parser() {
    assert_eq!(
        game_id(&app("steam steam://rungameid/4122860")),
        Some("4122860".into())
    );
    assert_eq!(
        game_id(&app(
            r#""/snap/bin/steam" -silent "steam://rungameid/4122860" %U"#
        )),
        Some("4122860".into())
    );
}

#[test]
fn unrelated_launchers_titles_and_client_exec_are_not_game_identity() {
    for exec in [
        "steam %U",
        "true steam://rungameid/4122860",
        "not-steam steam://rungameid/4122860",
        "sh -c steam steam://rungameid/4122860",
        "env steam steam://rungameid/4122860",
        "steam -applaunch 4122860",
        "steam \"steam://rungameid/4122860",
    ] {
        assert_eq!(game_id(&app(exec)), None, "{exec}");
    }
}

#[test]
fn malformed_ambiguous_and_non_numeric_uris_are_rejected() {
    for uri in [
        "steam://rungameid/",
        "steam://rungameid/../4122860",
        "steam://rungameid/+4122860",
        "steam://rungameid/4122860/",
        "steam://rungameid/4122860?x=1",
        "steam://rungameid/4122860#fragment",
        "steam://rungameid/１２３",
        "steam://rungameid/%%34",
        "steam://rungameid/123456789012345678901",
        "steam://run/4122860",
        "steam://rungameid/4122860 steam://rungameid/2076580",
    ] {
        assert_eq!(game_id(&app(&format!("steam {uri}"))), None, "{uri}");
    }
}

#[test]
fn every_fixed_native_snap_and_flatpak_root_can_supply_a_unique_square_jpeg() {
    let home = tempfile::tempdir().unwrap();
    for root in roots_under(home.path()) {
        let path = root.join(format!("appcache/librarycache/4122860/{HASH}.jpg"));
        jpeg(&path, 32, 32);
        let icon = cached_icon(&roots_under(home.path()), "4122860").unwrap();
        assert_eq!((icon.width, icon.height), (48, 48));
        fs::remove_file(path).unwrap();
    }
}

#[test]
fn modern_snap_artwork_precedes_an_older_native_legacy_icon() {
    let home = tempfile::tempdir().unwrap();
    let native = home.path().join(".local/share/Steam");
    let snap = home.path().join("snap/steam/common/.local/share/Steam");
    let legacy = native.join("appcache/librarycache/4122860_icon.jpg");
    let modern = snap.join(format!("appcache/librarycache/4122860/{HASH}.jpg"));
    jpeg(&legacy, 32, 32);
    fs::create_dir_all(modern.parent().unwrap()).unwrap();
    image::RgbImage::from_pixel(32, 32, image::Rgb([20, 210, 80]))
        .save(&modern)
        .unwrap();
    let roots = [native, snap];
    let current = square_icon(&modern).unwrap();
    assert_ne!(Some(current.clone()), square_icon(&legacy));
    assert_eq!(cached_icon(&roots, "4122860"), Some(current));
    fs::remove_file(modern).unwrap();
    assert_eq!(cached_icon(&roots, "4122860"), square_icon(&legacy));
}

#[test]
fn missing_cache_and_non_icon_artwork_do_not_supply_a_game_icon() {
    let root = tempfile::tempdir().unwrap();
    assert!(cached(root.path()).is_none());
    let directory = root.path().join("appcache/librarycache/4122860");
    for name in ["header.jpg", "library_600x900.jpg", "icon.jpg", "abc.jpg"] {
        jpeg(&directory.join(name), 32, 32);
    }
    jpeg(&directory.join(format!("nested/{HASH}.jpg")), 32, 32);
    assert!(cached(root.path()).is_none());
}

#[test]
fn multiple_hash_candidates_are_ambiguous_even_when_only_one_decodes() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("appcache/librarycache/4122860");
    jpeg(&directory.join(format!("{HASH}.jpg")), 32, 32);
    let other = directory.join(format!("{OTHER_HASH}.jpg"));
    jpeg(&other, 32, 32);
    assert!(cached(root.path()).is_none());
    fs::write(other, b"malformed jpeg").unwrap();
    assert!(cached(root.path()).is_none());
}

#[test]
fn legacy_icon_is_supported_but_rectangular_artwork_is_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("appcache/librarycache/4122860_icon.jpg");
    jpeg(&path, 32, 32);
    assert!(cached(root.path()).is_some());
    jpeg(&path, 64, 32);
    assert!(cached(root.path()).is_none());
    fs::remove_file(path).unwrap();
    jpeg(
        &root
            .path()
            .join(format!("appcache/librarycache/4122860/{HASH}.jpg")),
        64,
        32,
    );
    assert!(cached(root.path()).is_none());
}

#[test]
fn original_dimensions_reject_near_square_cache_but_allow_general_app_artwork() {
    let root = tempfile::tempdir().unwrap();
    let path = root
        .path()
        .join(format!("appcache/librarycache/4122860/{HASH}.jpg"));
    jpeg(&path, 1024, 1023);
    // Both sides round to 48: checking the thumbnail would accept this image.
    let general = super::super::decode_file(&path).unwrap();
    assert_eq!((general.width, general.height), (48, 48));
    assert!(cached(root.path()).is_none());
    let legacy = root.path().join("appcache/librarycache/4122860_icon.jpg");
    fs::rename(&path, &legacy).unwrap();
    assert!(cached(root.path()).is_none());
    jpeg(&path, 64, 32);
    let general = super::super::decode_file(&path).unwrap();
    assert_eq!((general.width, general.height), (48, 24));
    assert!(cached(root.path()).is_none());
}

#[test]
fn malformed_oversized_and_excessive_dimension_jpegs_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let path = root
        .path()
        .join(format!("appcache/librarycache/4122860/{HASH}.jpg"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"malformed jpeg").unwrap();
    assert!(cached(root.path()).is_none());
    fs::File::create(&path)
        .unwrap()
        .set_len(super::super::MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(cached(root.path()).is_none());
    jpeg(&path, super::super::MAX_DIMENSION + 1, 1);
    assert!(super::super::decode_file(&path).is_none());
    assert!(cached(root.path()).is_none());
}

#[test]
fn excessive_directory_entries_reject_the_whole_scan() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("appcache/librarycache/4122860");
    jpeg(&directory.join(format!("{HASH}.jpg")), 32, 32);
    for index in 0..MAX_CACHE_ENTRIES {
        fs::write(directory.join(format!("noise-{index}")), []).unwrap();
    }
    assert!(cached(root.path()).is_none());
}

#[test]
fn hash_named_symlinks_are_not_followed_and_accounts_are_not_searched() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("userdata/123/4122860/icon.jpg");
    jpeg(&outside, 32, 32);
    assert!(cached(root.path()).is_none());
    let directory = root.path().join("appcache/librarycache/4122860");
    fs::create_dir_all(&directory).unwrap();
    std::os::unix::fs::symlink(&outside, directory.join(format!("{HASH}.jpg"))).unwrap();
    assert!(cached(root.path()).is_none());
}
