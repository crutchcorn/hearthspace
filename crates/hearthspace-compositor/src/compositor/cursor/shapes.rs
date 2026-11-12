//! Embedded artwork for the cursor-shape protocol's additional semantic shapes.
use super::CursorIcon;

pub(super) fn asset(icon: CursorIcon) -> Option<&'static [u8]> {
    macro_rules! assets {
        ($($variant:ident => $name:literal),* $(,)?) => {
            match icon {
                $(CursorIcon::$variant => Some(include_bytes!(concat!(
                    "../../../../../assets/cursors/adwaita/", $name
                )).as_slice()),)*
                _ => None,
            }
        };
    }
    assets! {
        ContextMenu => "context-menu",
        Help => "help",
        Pointer => "pointer",
        Progress => "progress",
        Wait => "wait",
        Cell => "cell",
        Crosshair => "crosshair",
        Text => "text",
        VerticalText => "vertical-text",
        Alias => "alias",
        Copy => "copy",
        Move => "move",
        NoDrop => "no-drop",
        NotAllowed => "not-allowed",
        EResize => "e-resize",
        NResize => "n-resize",
        NeResize => "ne-resize",
        NwResize => "nw-resize",
        SResize => "s-resize",
        SeResize => "se-resize",
        SwResize => "sw-resize",
        WResize => "w-resize",
        ColResize => "col-resize",
        RowResize => "row-resize",
        AllScroll => "all-scroll",
        ZoomIn => "zoom-in",
        ZoomOut => "zoom-out",
        DndAsk => "dnd-ask",
        AllResize => "all-resize",
    }
}
