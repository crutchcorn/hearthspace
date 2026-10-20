use super::*;
use std::os::unix::fs::PermissionsExt as _;

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn photo(path: &Path, width: u32, height: u32, color: [u8; 4]) {
    image::RgbaImage::from_pixel(width, height, image::Rgba(color))
        .save(path)
        .unwrap();
}

fn assert_image(image: &ImageBrush, color: [u8; 4]) {
    assert_eq!((image.image.width, image.image.height), (WIDTH, HEIGHT));
    assert_eq!(image.image.data.as_ref().len(), PIXEL_BYTES);
    assert!(
        image
            .image
            .data
            .as_ref()
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| *pixel == color)
    );
}

#[test]
fn a_new_settings_instance_reuses_the_persisted_thumbnail() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let cache_directory = directory.path().join("cache");
    photo(&source, 800, 600, RED);
    let mut first = ThumbnailCache::with_directory(cache_directory.clone());
    assert_image(&first.load(&source).unwrap(), RED);
    let cache_path = cache_directory.join(CACHE_FILE);
    let written = fs::metadata(&cache_path).unwrap();
    assert_eq!(written.permissions().mode() & 0o777, 0o600);
    assert!(written.len() <= MAX_CACHE_BYTES);

    let mut restarted = ThumbnailCache::with_directory(cache_directory.clone());
    assert_image(&restarted.load(&source).unwrap(), RED);
    let reused = fs::metadata(&cache_path).unwrap();
    assert_eq!(
        written.ino(),
        reused.ino(),
        "cache was unnecessarily rebuilt"
    );
    assert_eq!(written.modified().unwrap(), reused.modified().unwrap());
    assert_eq!(fs::read_dir(cache_directory).unwrap().count(), 1);
}

#[test]
fn replacement_with_the_same_path_and_timestamp_invalidates_both_caches() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let replacement = directory.path().join("replacement.png");
    let cache_directory = directory.path().join("cache");
    photo(&source, 8, 8, RED);
    let timestamp = fs::metadata(&source).unwrap().modified().unwrap();
    let mut cache = ThumbnailCache::with_directory(cache_directory.clone());
    assert_image(&cache.load(&source).unwrap(), RED);
    photo(&replacement, 8, 8, BLUE);
    File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_modified(timestamp)
        .unwrap();
    fs::rename(replacement, &source).unwrap();

    assert_image(&cache.load(&source).unwrap(), BLUE);
    let mut restarted = ThumbnailCache::with_directory(cache_directory);
    assert_image(&restarted.load(&source).unwrap(), BLUE);
}

#[test]
fn deleted_source_never_reuses_a_persisted_photo_and_recreation_recovers() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let cache_directory = directory.path().join("cache");
    photo(&source, 8, 8, RED);
    let mut cache = ThumbnailCache::with_directory(cache_directory.clone());
    assert_image(&cache.load(&source).unwrap(), RED);
    fs::remove_file(&source).unwrap();
    assert!(cache.load(&source).is_none());
    assert!(
        ThumbnailCache::with_directory(cache_directory)
            .load(&source)
            .is_none()
    );
    photo(&source, 8, 8, BLUE);
    assert_image(&cache.load(&source).unwrap(), BLUE);
}

#[test]
fn corrupt_or_oversized_cache_is_replaced_from_the_original() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let cache_directory = directory.path().join("cache");
    photo(&source, 8, 8, RED);
    let mut cache = ThumbnailCache::with_directory(cache_directory.clone());
    cache.load(&source).unwrap();
    let cache_path = cache_directory.join(CACHE_FILE);
    fs::write(&cache_path, b"truncated PNG").unwrap();
    let mut restarted = ThumbnailCache::with_directory(cache_directory.clone());
    assert_image(&restarted.load(&source).unwrap(), RED);
    assert!(read_cached(&cache_path, &Key::read(&source).identity().unwrap()).is_some());
    File::create(&cache_path)
        .unwrap()
        .set_len(MAX_CACHE_BYTES + 1)
        .unwrap();
    let mut restarted_again = ThumbnailCache::with_directory(cache_directory);
    assert_image(&restarted_again.load(&source).unwrap(), RED);
    assert!(fs::metadata(cache_path).unwrap().len() <= MAX_CACHE_BYTES);
}

#[test]
fn another_source_cannot_reuse_the_previous_photo() {
    let directory = tempfile::tempdir().unwrap();
    let red = directory.path().join("red.png");
    let blue = directory.path().join("blue.png");
    let cache_directory = directory.path().join("cache");
    photo(&red, 8, 8, RED);
    photo(&blue, 8, 8, BLUE);
    ThumbnailCache::with_directory(cache_directory.clone())
        .load(&red)
        .unwrap();
    assert_image(
        &ThumbnailCache::with_directory(cache_directory)
            .load(&blue)
            .unwrap(),
        BLUE,
    );
}

#[test]
fn an_unwritable_cache_keeps_the_photo_available_in_memory() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let blocked_directory = directory.path().join("not-a-directory");
    photo(&source, 8, 8, RED);
    // This fails even for privileged test runners, unlike chmod on a directory.
    fs::write(&blocked_directory, b"keep this file").unwrap();
    let mut cache = ThumbnailCache::with_directory(blocked_directory.clone());
    assert_image(&cache.load(&source).unwrap(), RED);
    assert_image(&cache.load(&source).unwrap(), RED);
    assert_eq!(fs::read(blocked_directory).unwrap(), b"keep this file");
}

#[test]
fn unreadable_invalid_and_oversized_sources_fall_back_and_recover() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("photo.png");
    let mut cache = ThumbnailCache::with_directory(directory.path().join("cache"));
    assert!(cache.load(&source).is_none());
    fs::write(&source, b"not a PNG").unwrap();
    assert!(cache.load(&source).is_none());
    assert!(cache.load(&source).is_none());
    File::create(&source)
        .unwrap()
        .set_len(MAX_FILE_BYTES + 1)
        .unwrap();
    assert!(cache.load(&source).is_none());
    photo(&source, MAX_DIMENSION + 1, 1, RED);
    assert!(cache.load(&source).is_none());
    photo(&source, 8, 8, BLUE);
    assert_image(&cache.load(&source).unwrap(), BLUE);
}

#[test]
fn extremely_wide_png_is_cropped_before_upscaling() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("wide.png");
    photo(&source, MAX_DIMENSION, 1, RED);
    let mut cache = ThumbnailCache::with_directory(directory.path().join("cache"));
    assert_image(&cache.load(&source).unwrap(), RED);
}

#[test]
fn extremely_tall_png_is_cropped_before_upscaling() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("tall.png");
    photo(&source, 1, MAX_DIMENSION, BLUE);
    let mut cache = ThumbnailCache::with_directory(directory.path().join("cache"));
    assert_image(&cache.load(&source).unwrap(), BLUE);
}
