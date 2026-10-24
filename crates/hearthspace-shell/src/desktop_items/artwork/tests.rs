use super::*;
use lofty::{
    config::{ParseOptions, WriteOptions},
    file::AudioFile,
    flac::FlacFile,
    ogg::OggPictureStorage,
    picture::{MimeType, Picture, PictureType},
};
use std::io::Cursor;

// 100 ms of generated mono silence; no user music or artwork is a test fixture.
const SILENCE: &[u8] = include_bytes!("fixtures/silence.flac");

fn write_cover(path: &Path, color: [u8; 3]) {
    let mut flac = FlacFile::read_from(&mut Cursor::new(SILENCE), ParseOptions::new()).unwrap();
    let mut jpeg = Cursor::new(Vec::new());
    image::RgbImage::from_pixel(600, 600, image::Rgb(color))
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    let picture = Picture::unchecked(jpeg.into_inner())
        .pic_type(PictureType::CoverFront)
        .mime_type(MimeType::Jpeg)
        .build();
    flac.insert_picture(picture, None).unwrap();
    fs::write(path, SILENCE).unwrap();
    flac.save_to_path(path, WriteOptions::default()).unwrap();
}

fn files(path: &Path) -> Files {
    BTreeMap::from([("file:song".into(), path.to_owned())])
}

#[test]
fn embedded_jpeg_cover_appears_without_an_existing_thumbnail_and_reloads() {
    let directory = tempfile::tempdir().unwrap();
    let song = directory.path().join("song.flac");
    write_cover(&song, [220, 20, 30]);
    let paths = files(&song);
    let mut cache = Cache::default();
    let first = cache.refresh(&paths);
    let red = &first["file:song"];
    assert_eq!((red.width, red.height), (64, 64));
    assert!(
        red.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[0] > 200 && pixel[1] < 40)
    );
    let again = cache.refresh(&paths);
    assert!(
        Arc::ptr_eq(red, &again["file:song"]),
        "unchanged covers reuse their decoded pixels"
    );

    // Atomic replacement also covers equal-size files and preserved mtimes.
    let replacement = directory.path().join("replacement.flac");
    write_cover(&replacement, [20, 30, 220]);
    fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_modified(fs::metadata(&song).unwrap().modified().unwrap())
        .unwrap();
    fs::rename(&replacement, &song).unwrap();
    let updated = cache.refresh(&paths);
    let blue = &updated["file:song"];
    assert!(
        blue.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[2] > 200 && pixel[0] < 40)
    );
    assert!(!Arc::ptr_eq(red, blue));
}

#[test]
fn flac_without_artwork_keeps_its_mime_icon_and_recovers_after_cover_is_added() {
    let directory = tempfile::tempdir().unwrap();
    let song = directory.path().join("song.flac");
    fs::write(&song, SILENCE).unwrap();
    let source = Source::read(&song).unwrap();
    assert!(source.icons.iter().any(|icon| icon == "audio-x-generic"));
    let expected = source
        .icons
        .iter()
        .find_map(|name| {
            app_icon::resolve(&IconSource {
                name: Some(name.clone()),
                ..Default::default()
            })
        })
        .expect("Adwaita provides the MIME icon used by the desktop");
    let paths = files(&song);
    let mut cache = Cache::default();
    let fallback = cache.refresh(&paths);
    assert_eq!(*fallback["file:song"], expected);
    assert!(cache.entries[&song].preview_loaded);
    assert!(Arc::ptr_eq(
        &fallback["file:song"],
        &cache.refresh(&paths)["file:song"]
    ));

    write_cover(&song, [20, 220, 30]);
    let art = cache.refresh(&paths);
    assert!(
        art["file:song"]
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| pixel[1] > 200 && pixel[0] < 40)
    );
    fs::write(&song, SILENCE).unwrap();
    assert_eq!(*cache.refresh(&paths)["file:song"], expected);
    fs::remove_file(&song).unwrap();
    assert!(cache.refresh(&paths).is_empty());
    assert!(cache.entries.is_empty());
}

#[test]
fn a_large_audio_payload_does_not_prevent_reading_small_leading_cover_metadata() {
    let directory = tempfile::tempdir().unwrap();
    let song = directory.path().join("large.flac");
    write_cover(&song, [220, 20, 30]);
    fs::File::options()
        .write(true)
        .open(&song)
        .unwrap()
        .set_len(8 * 1024 * 1024 * 1024)
        .unwrap();
    assert!(cover::flac(&song).is_some());
}

#[test]
fn gnome_large_thumbnail_requires_the_matching_validity_attribute() {
    let directory = tempfile::tempdir().unwrap();
    let thumbnail = directory.path().join("large.png");
    image::RgbaImage::from_pixel(256, 256, image::Rgba([20, 80, 180, 255]))
        .save(&thumbnail)
        .unwrap();
    let info = gio::FileInfo::new();
    info.set_attribute_byte_string("thumbnail::path-large", thumbnail.to_str().unwrap());
    info.set_attribute_boolean("thumbnail::is-valid", true);
    assert!(
        valid_thumbnail(&info).is_none(),
        "normal validity cannot authorize a stale large thumbnail"
    );
    info.set_attribute_boolean("thumbnail::is-valid-large", true);
    assert_eq!(valid_thumbnail(&info).unwrap().0, thumbnail);
    info.set_attribute_boolean("thumbnail::is-valid-large", false);
    assert!(valid_thumbnail(&info).is_none());
}

#[test]
fn a_valid_platform_thumbnail_takes_precedence_over_embedded_art() {
    let directory = tempfile::tempdir().unwrap();
    let song = directory.path().join("song.flac");
    write_cover(&song, [220, 20, 30]);
    let thumbnail = directory.path().join("large.png");
    image::RgbaImage::from_pixel(128, 64, image::Rgba([20, 80, 180, 255]))
        .save(&thumbnail)
        .unwrap();
    let mut source = Source::read(&song).unwrap();
    source.thumbnail = Some((thumbnail.clone(), Stamp::read(&thumbnail).unwrap()));
    let image = source.preview(&song).unwrap();
    assert_eq!((image.width, image.height), (64, 32));
    assert_eq!(&image.rgba[..4], &[20, 80, 180, 255]);
    fs::write(thumbnail, "corrupt preview").unwrap();
    let image = source.preview(&song).unwrap();
    assert!(
        image.rgba[0] > 200,
        "corrupt cached PNG falls back to the embedded cover"
    );
}

#[test]
fn a_thumbnail_older_than_the_last_file_replacement_cannot_hide_updated_artwork() {
    let directory = tempfile::tempdir().unwrap();
    let song = directory.path().join("song.flac");
    write_cover(&song, [220, 20, 30]);
    let thumbnail = directory.path().join("old.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([20, 80, 180, 255]))
        .save(&thumbnail)
        .unwrap();
    fs::File::options()
        .write(true)
        .open(&thumbnail)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH)
        .unwrap();
    let mut source = Source::read(&song).unwrap();
    source.thumbnail = Some((thumbnail.clone(), Stamp::read(&thumbnail).unwrap()));
    assert!(source.preview(&song).unwrap().rgba[0] > 200);
}

#[test]
fn preview_work_is_batched_and_removed_files_do_not_accumulate_in_the_cache() {
    let directory = tempfile::tempdir().unwrap();
    let paths: Files = (0..=MAX_PREVIEWS_PER_POLL)
        .map(|index| {
            let path = directory.path().join(format!("{index}.flac"));
            fs::write(&path, SILENCE).unwrap();
            (format!("file:{index}"), path)
        })
        .collect();
    let mut cache = Cache::default();
    cache.refresh(&paths);
    assert_eq!(
        cache
            .entries
            .values()
            .filter(|entry| entry.preview_loaded)
            .count(),
        MAX_PREVIEWS_PER_POLL
    );
    cache.refresh(&paths);
    assert!(cache.entries.values().all(|entry| entry.preview_loaded));
    cache.refresh(&Files::new());
    assert!(cache.entries.is_empty());
}
