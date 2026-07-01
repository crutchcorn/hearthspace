use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    utils::Transform,
};
use xcursor::parser::parse_xcursor;

#[cfg(feature = "winit")]
pub(crate) use smithay::reexports::winit::window::CursorIcon;

#[cfg(not(feature = "winit"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorIcon {
    Default,
    NsResize,
    EwResize,
    NwseResize,
    NeswResize,
}

pub(super) struct SoftwareCursor {
    pub(super) buffer: MemoryRenderBuffer,
    pub(super) hotspot: (i32, i32),
}

const DEFAULT_CURSOR: &[u8] = include_bytes!("../../assets/cursors/default");
const DESIRED_CURSOR_SIZE: u32 = 24;

pub(super) fn standard_software_cursor() -> SoftwareCursor {
    let images =
        parse_xcursor(DEFAULT_CURSOR).expect("packaged default cursor must be valid Xcursor data");
    let image = images
        .iter()
        .min_by_key(|image| image.size.abs_diff(DESIRED_CURSOR_SIZE))
        .expect("packaged default cursor must contain at least one image");

    let width = i32::try_from(image.width).expect("cursor width must fit in i32");
    let height = i32::try_from(image.height).expect("cursor height must fit in i32");
    let hotspot = (
        i32::try_from(image.xhot).expect("cursor x hotspot must fit in i32"),
        i32::try_from(image.yhot).expect("cursor y hotspot must fit in i32"),
    );

    let buffer = MemoryRenderBuffer::from_slice(
        &image.pixels_rgba,
        Fourcc::Abgr8888,
        (width, height),
        1,
        Transform::Normal,
        None,
    );

    SoftwareCursor { buffer, hotspot }
}
