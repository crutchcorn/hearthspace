//! UI-independent image data shared by application icons and file previews.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    /// Unpremultiplied RGBA, one four-byte pixel per image coordinate.
    pub rgba: Vec<u8>,
}

/// A named icon with optional private theme and raster fallback.
/// Consumers own cache identities, invalidation and UI image conversion.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IconSource {
    pub name: Option<String>,
    pub theme_path: Option<String>,
    pub pixmap: Option<RasterImage>,
}
