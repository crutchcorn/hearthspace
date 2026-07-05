use smithay::{
    backend::{allocator::Fourcc, renderer::element::memory::MemoryRenderBuffer},
    input::pointer::CursorIcon as SmithayCursorIcon,
    utils::Transform,
};
use xcursor::parser::parse_xcursor;

#[cfg(feature = "winit")]
pub(crate) use cursor_icon::CursorIcon;

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
    /// The named shape this buffer was rasterized for, so the render path can
    /// rebuild the buffer only when the desired shape changes.
    pub(super) icon: CursorIcon,
    pub(super) buffer: MemoryRenderBuffer,
    pub(super) hotspot: (i32, i32),
}

pub(in crate::compositor) struct CursorImage {
    pub(in crate::compositor) width: u32,
    pub(in crate::compositor) height: u32,
    pub(in crate::compositor) hotspot: (i32, i32),
    pub(in crate::compositor) pixels_rgba: Vec<u8>,
}

const DEFAULT_CURSOR: &[u8] = include_bytes!("../../../../assets/cursors/default");
const NS_RESIZE_CURSOR: &[u8] = include_bytes!("../../../../assets/cursors/ns-resize");
const EW_RESIZE_CURSOR: &[u8] = include_bytes!("../../../../assets/cursors/ew-resize");
const NWSE_RESIZE_CURSOR: &[u8] = include_bytes!("../../../../assets/cursors/nwse-resize");
const NESW_RESIZE_CURSOR: &[u8] = include_bytes!("../../../../assets/cursors/nesw-resize");
const DESIRED_CURSOR_SIZE: u32 = 24;

pub(super) fn cursor_icon_from_smithay(icon: SmithayCursorIcon) -> CursorIcon {
    match icon {
        SmithayCursorIcon::Default => CursorIcon::Default,
        SmithayCursorIcon::NsResize => CursorIcon::NsResize,
        SmithayCursorIcon::EwResize => CursorIcon::EwResize,
        SmithayCursorIcon::NwseResize => CursorIcon::NwseResize,
        SmithayCursorIcon::NeswResize => CursorIcon::NeswResize,
        _ => CursorIcon::Default,
    }
}

pub(super) fn standard_software_cursor() -> SoftwareCursor {
    software_cursor_for_icon(CursorIcon::Default)
}

/// Rasterize the software cursor buffer for a named shape at native (unscaled)
/// size. The whole scene is rescaled by the output scale at render time, so this
/// buffer stays scale-1 and its hotspot is expressed in native cursor pixels.
pub(super) fn software_cursor_for_icon(icon: CursorIcon) -> SoftwareCursor {
    let image = cursor_image_for_icon(icon);
    let width = i32::try_from(image.width).expect("cursor width must fit in i32");
    let height = i32::try_from(image.height).expect("cursor height must fit in i32");

    let buffer = MemoryRenderBuffer::from_slice(
        &image.pixels_rgba,
        Fourcc::Abgr8888,
        (width, height),
        1,
        Transform::Normal,
        None,
    );

    SoftwareCursor {
        icon,
        buffer,
        hotspot: image.hotspot,
    }
}

pub(in crate::compositor) fn cursor_image_for_icon(icon: CursorIcon) -> CursorImage {
    cursor_image_for_icon_at_size(icon, DESIRED_CURSOR_SIZE)
}

pub(in crate::compositor) fn cursor_image_for_icon_at_size(
    icon: CursorIcon,
    desired_size: u32,
) -> CursorImage {
    match icon {
        CursorIcon::Default => cursor_image_from_xcursor(DEFAULT_CURSOR, desired_size),
        CursorIcon::NsResize => cursor_image_from_xcursor(NS_RESIZE_CURSOR, desired_size),
        CursorIcon::EwResize => cursor_image_from_xcursor(EW_RESIZE_CURSOR, desired_size),
        CursorIcon::NwseResize => cursor_image_from_xcursor(NWSE_RESIZE_CURSOR, desired_size),
        CursorIcon::NeswResize => cursor_image_from_xcursor(NESW_RESIZE_CURSOR, desired_size),
        #[cfg(feature = "winit")]
        _ => cursor_image_from_xcursor(DEFAULT_CURSOR, desired_size),
    }
}

fn cursor_image_from_xcursor(contents: &[u8], desired_size: u32) -> CursorImage {
    let images = parse_xcursor(contents).expect("packaged cursor must be valid Xcursor data");
    let image = images
        .iter()
        .min_by_key(|image| image.size.abs_diff(desired_size))
        .expect("packaged cursor must contain at least one image");

    let hotspot = (
        i32::try_from(image.xhot).expect("cursor x hotspot must fit in i32"),
        i32::try_from(image.yhot).expect("cursor y hotspot must fit in i32"),
    );

    CursorImage {
        width: image.width,
        height: image.height,
        hotspot,
        pixels_rgba: image.pixels_rgba.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMED_ICONS: [CursorIcon; 5] = [
        CursorIcon::Default,
        CursorIcon::NsResize,
        CursorIcon::EwResize,
        CursorIcon::NwseResize,
        CursorIcon::NeswResize,
    ];

    #[test]
    fn every_named_cursor_builds_a_software_buffer() {
        // The software cursor path is now used by every backend (headless, the
        // native software fallback, and client-surface presentation), so each
        // packaged shape must rasterize without panicking.
        for icon in NAMED_ICONS {
            let cursor = software_cursor_for_icon(icon);
            assert_eq!(cursor.icon, icon);
        }
    }

    #[test]
    fn cursor_images_carry_hotspot_and_pixels() {
        for icon in NAMED_ICONS {
            let image = cursor_image_for_icon(icon);
            assert!(image.width > 0 && image.height > 0);
            assert_eq!(
                image.pixels_rgba.len(),
                (image.width * image.height * 4) as usize
            );
            assert!(image.hotspot.0 >= 0 && image.hotspot.0 as u32 <= image.width);
            assert!(image.hotspot.1 >= 0 && image.hotspot.1 as u32 <= image.height);
        }
    }
}
