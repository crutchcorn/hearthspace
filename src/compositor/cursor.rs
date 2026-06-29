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
