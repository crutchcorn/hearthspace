//! Worker-owned background thumbnail cache. No file I/O belongs in the view.
use std::{
    fmt::Write as _,
    fs::{self, File, Metadata, OpenOptions},
    io::{self, Cursor, Read},
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use image::GenericImageView as _;
use xilem::masonry::peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat};

pub(crate) const WIDTH: u32 = 336;
pub(crate) const HEIGHT: u32 = 256;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_DIMENSION: u32 = 16_384;
const MAX_CACHE_BYTES: u64 = 512 * 1024;
const PIXEL_BYTES: usize = (WIDTH * HEIGHT * 4) as usize;
const CACHE_FILE: &str = "background-preview-v1.png";
const IDENTITY_KEYWORD: &str = "Hearthspace source";

#[derive(Clone, PartialEq)]
struct Stamp {
    device: u64,
    inode: u64,
    length: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}

impl Stamp {
    fn read(metadata: &Metadata) -> Option<Self> {
        (metadata.is_file() && metadata.len() <= MAX_FILE_BYTES).then(|| Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            length: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}

#[derive(PartialEq)]
struct Key {
    path: PathBuf,
    stamp: Option<Stamp>,
}

impl Key {
    fn read(path: &Path) -> Self {
        Self {
            path: path.to_owned(),
            stamp: fs::metadata(path).ok().as_ref().and_then(Stamp::read),
        }
    }

    fn identity(&self) -> Option<String> {
        let stamp = self.stamp.as_ref()?;
        let mut identity = format!(
            "v1;center-crop-triangle;{WIDTH}x{HEIGHT};{};{};{};{},{};{},{};",
            stamp.device,
            stamp.inode,
            stamp.length,
            stamp.modified.0,
            stamp.modified.1,
            stamp.changed.0,
            stamp.changed.1,
        );
        // Hex preserves non-UTF-8 paths without collisions in the PNG text field.
        for byte in self.path.as_os_str().as_bytes() {
            let _ = write!(identity, "{byte:02x}");
        }
        Some(identity)
    }
}

struct Preview {
    key: Key,
    image: Option<ImageBrush>,
}

pub(crate) struct ThumbnailCache {
    directory: Option<PathBuf>,
    memory: Option<Preview>,
}

impl ThumbnailCache {
    pub(crate) fn new() -> Self {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .filter(|path| path.is_absolute())
                    .map(|path| path.join(".cache"))
            });
        match base {
            Some(path) => Self::with_directory(path.join("hearthspace")),
            None => Self {
                directory: None,
                memory: None,
            },
        }
    }

    pub(crate) fn with_directory(directory: PathBuf) -> Self {
        Self {
            directory: Some(directory),
            memory: None,
        }
    }

    /// Called only by the preview worker; brushes share their pixel allocation.
    pub(crate) fn load(&mut self, path: &Path) -> Option<ImageBrush> {
        // A replacement during decoding gets one retry, never an unbounded loop.
        for _ in 0..2 {
            let key = Key::read(path);
            if let Some(preview) = self.memory.as_ref()
                && preview.key == key
            {
                return preview.image.clone();
            }
            let identity = key.identity();
            let cached = identity.as_deref().and_then(|identity| {
                read_cached(&self.directory.as_ref()?.join(CACHE_FILE), identity)
            });
            let from_disk = cached.is_some();
            let image = cached.or_else(|| decode_source(&key));
            if Key::read(path) != key {
                continue;
            }
            if !from_disk
                && let (Some(directory), Some(identity), Some(image)) =
                    (&self.directory, identity.as_deref(), image.as_ref())
            {
                // An unwritable cache must not hide a successfully decoded photo.
                let _ = write_cached(directory, identity, image);
            }
            if Key::read(path) != key {
                continue;
            }
            self.memory = Some(Preview {
                key,
                image: image.clone(),
            });
            return image;
        }
        None
    }
}

fn bounded_read(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= limit).then_some(bytes)
}

fn decode_source(key: &Key) -> Option<ImageBrush> {
    let stamp = key.stamp.as_ref()?;
    let mut file = File::open(&key.path).ok()?;
    if Stamp::read(&file.metadata().ok()?).as_ref() != Some(stamp) {
        return None;
    }
    let mut bytes = Vec::new();
    (&mut file)
        .take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() as u64 > MAX_FILE_BYTES
        || Stamp::read(&file.metadata().ok()?).as_ref() != Some(stamp)
    {
        return None;
    }
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), image::ImageFormat::Png);
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().ok()?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    // Crop before resizing: cover-resize can allocate gigabytes for a thin PNG.
    // Borrow the crop so even a large source does not need a second full copy.
    let (crop_width, crop_height) =
        if u64::from(width) * u64::from(HEIGHT) > u64::from(height) * u64::from(WIDTH) {
            ((height * WIDTH / HEIGHT).max(1).min(width), height)
        } else {
            (width, (width * HEIGHT / WIDTH).max(1).min(height))
        };
    let crop = image.view(
        (width - crop_width) / 2,
        (height - crop_height) / 2,
        crop_width,
        crop_height,
    );
    let pixels =
        image::imageops::resize(&*crop, WIDTH, HEIGHT, image::imageops::FilterType::Triangle);
    Some(brush(pixels.into_raw()))
}

fn brush(pixels: Vec<u8>) -> ImageBrush {
    ImageData {
        data: pixels.into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: WIDTH,
        height: HEIGHT,
    }
    .into()
}

fn read_cached(path: &Path, identity: &str) -> Option<ImageBrush> {
    let bytes = bounded_read(path, MAX_CACHE_BYTES)?;
    let decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: 2 * 1024 * 1024,
        },
    );
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    if info.width != WIDTH
        || info.height != HEIGHT
        || info.color_type != png::ColorType::Rgba
        || info.bit_depth != png::BitDepth::Eight
        || info.animation_control.is_some()
        || !info.utf8_text.iter().any(|chunk| {
            chunk.keyword == IDENTITY_KEYWORD
                && !chunk.compressed
                && chunk.get_text().is_ok_and(|text| text == identity)
        })
        || reader.output_buffer_size()? != PIXEL_BYTES
    {
        return None;
    }
    let mut pixels = vec![0; PIXEL_BYTES];
    let output = reader.next_frame(&mut pixels).ok()?;
    if output.buffer_size() != PIXEL_BYTES {
        return None;
    }
    reader.finish().ok()?;
    Some(brush(pixels))
}

fn write_cached(directory: &Path, identity: &str, image: &ImageBrush) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let (temporary, mut file) = temporary_file(directory)?;
    let result = (|| {
        let mut encoder = png::Encoder::new(&mut file, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.add_itxt_chunk(IDENTITY_KEYWORD.to_owned(), identity.to_owned())?;
        let mut writer = encoder.write_header()?;
        writer.write_image_data(image.image.data.as_ref())?;
        writer.finish()?;
        file.sync_all()?;
        fs::rename(&temporary, directory.join(CACHE_FILE))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn temporary_file(directory: &Path) -> io::Result<(PathBuf, File)> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..16 {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(
            ".background-preview-{}-{sequence}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not create a background preview cache file",
    ))
}

#[cfg(test)]
mod tests;
