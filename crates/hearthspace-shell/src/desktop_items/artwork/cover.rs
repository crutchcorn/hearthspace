//! Bounded decoders for file previews. Lofty owns the FLAC metadata format.
use std::{
    fs::{File, OpenOptions},
    io::{self, BufReader, Cursor, Read, Seek, SeekFrom},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

use lofty::{
    config::{GlobalOptions, ParseOptions, apply_global_options},
    file::AudioFile,
    flac::FlacFile,
    ogg::OggPictureStorage,
    picture::PictureType,
};

use crate::tray::TrayPixmap;

const MAX_METADATA_BYTES: u64 = 16 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const ICON_SIZE: u32 = 64;

fn open_regular(path: &Path) -> Option<File> {
    // A file can be replaced after directory discovery. NONBLOCK prevents a
    // replacement FIFO from hanging the worker before we can inspect its fd.
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(rustix::fs::OFlags::NONBLOCK.bits() as i32)
        .open(path)
        .ok()?;
    file.metadata().ok()?.is_file().then_some(file)
}

pub(super) fn flac(path: &Path) -> Option<TrayPixmap> {
    let file = open_regular(path)?;
    let length = file.metadata().ok()?.len();
    let reader = LimitedReader::new(file, length);
    // GlobalOptions are thread-local, and this worker's only Lofty consumer is
    // cover extraction. Disable tags/properties so audio frames are never read.
    apply_global_options(GlobalOptions::new().allocation_limit(MAX_IMAGE_BYTES as usize));
    let file = FlacFile::read_from(
        &mut BufReader::new(reader),
        ParseOptions::new()
            .read_properties(false)
            .read_tags(false)
            .read_cover_art(true),
    )
    .ok()?;
    let pictures = file.pictures();
    // Prefer the front cover even if a back cover/icon appears first. Broken
    // image data is skipped so another valid embedded picture can still appear.
    pictures
        .iter()
        .filter(|(picture, _)| picture.pic_type() == PictureType::CoverFront)
        .chain(
            pictures
                .iter()
                .filter(|(picture, _)| picture.pic_type() != PictureType::CoverFront),
        )
        .take(16)
        .find_map(|(picture, _)| image_bytes(picture.data()))
}

pub(super) fn image_file(path: &Path) -> Option<TrayPixmap> {
    let file = open_regular(path)?;
    if file.metadata().ok()?.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    image_bytes(&bytes)
}

fn image_bytes(bytes: &[u8]) -> Option<TrayPixmap> {
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return None;
    }
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    // Covers and freedesktop thumbnails require no animation/SVG codecs.
    if !matches!(
        reader.format(),
        Some(image::ImageFormat::Png | image::ImageFormat::Jpeg)
    ) {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .decode()
        .ok()?
        .thumbnail(ICON_SIZE, ICON_SIZE)
        .to_rgba8();
    Some(TrayPixmap {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
    })
}

/// Limit total reads, accessible prefix and parser operations. Huge audio files
/// remain eligible because only the bounded leading metadata is inspected.
struct LimitedReader<R> {
    inner: R,
    length: u64,
    position: u64,
    remaining: u64,
    operations: usize,
}

impl<R> LimitedReader<R> {
    fn new(inner: R, length: u64) -> Self {
        Self {
            inner,
            length,
            position: 0,
            remaining: MAX_METADATA_BYTES,
            operations: 65_536,
        }
    }

    fn operation(&mut self) -> io::Result<()> {
        self.operations = self
            .operations
            .checked_sub(1)
            .ok_or_else(|| io::Error::other("file preview operation limit"))?;
        Ok(())
    }
}

impl<R: Read> Read for LimitedReader<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.operation()?;
        let available = self
            .remaining
            .min(MAX_METADATA_BYTES.saturating_sub(self.position));
        if available == 0 && !bytes.is_empty() {
            return Err(io::Error::other("file preview read limit"));
        }
        let count = bytes.len().min(available as usize);
        let read = self.inner.read(&mut bytes[..count])?;
        self.remaining -= read as u64;
        self.position += read as u64;
        Ok(read)
    }
}

impl<R: Seek> Seek for LimitedReader<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        self.operation()?;
        let position = match from {
            SeekFrom::Start(position) => Some(position),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.length.checked_add_signed(offset),
        }
        .filter(|position| *position <= MAX_METADATA_BYTES)
        .ok_or_else(|| io::Error::other("file preview seek limit"))?;
        self.position = self.inner.seek(SeekFrom::Start(position))?;
        Ok(self.position)
    }
}

#[cfg(test)]
mod tests;
