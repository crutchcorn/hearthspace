//! Exact geometry from hearthspace-ui-experiments/src/icons/canvas.tsx.
use super::{Geometry, Icon};

pub(super) fn append(icon: Icon, _: bool, g: &mut Geometry) -> bool {
    match icon {
        Icon::Pan => {
            g.path(
                "M12 3v5M9 6l3-3 3 3M12 16v5m-3-3 3 3 3-3M3 12h5m-2-3-3 3 3 3m10-3h5m-3-3 3 3-3 3",
            );
            g.circle(12.0, 12.0, 1.5, false);
        }
        Icon::Lasso => {
            g.ellipse(10.5, 7.75, 7.65, 4.95, -18.0, false);
            g.circle(5.1, 11.35, 1.8, false);
            g.path("M5.1 13.15v3.35a3 3 0 0 0 3 3h1.4");
            g.path("m14.2 13.4 7.2 4.4-3.2.8-1.6 2.8Z");
        }
        Icon::Overview => {
            g.path("M3.5 8V3.5H8m8 0h4.5V8m0 8v4.5H16m-8 0H3.5V16");
            g.rect(7.0, 7.0, 9.0, 8.0, 1.0, false);
            g.path("M10 10h3");
        }
        Icon::Cursor => {
            g.path("M4 3.5 19.5 12l-7 2-3 6Z");
        }
        Icon::FrameLayout => {
            g.rect(3.5, 3.5, 17.0, 17.0, 1.0, false);
            g.path("M8 3.5v17M3.5 8h17");
            g.rect(11.0, 11.0, 6.5, 6.0, 0.75, false);
        }
        Icon::Pencil => {
            g.path("m4 20 1-5L16 4l4 4L9 19Z");
            g.path("m13 7 4 4M5 15l4 4");
        }
        Icon::Search => {
            g.circle(9.5, 9.5, 6.0, false);
            g.path("m14 14 6.5 6.5");
        }
        Icon::RectangleSelect => {
            g.path("M3.5 8V3.5H8m3.5 0h3m4 0V8M3.5 11.5v3m0 4H8");
            g.path("m11.5 10.5 9 5.5-4 1-2 3.5Z");
        }
        Icon::Layers => {
            g.path("m4 8 8-4 8 4-8 4Z");
            g.path("m4 14 8 4 8-4");
        }
        Icon::BackgroundLayer => {
            g.rect(3.5, 3.5, 17.0, 17.0, 1.0, false);
            g.path("m7 11 4-4m-4 10L17 7m-4 10 4-4");
        }
        Icon::Ungroup => {
            g.rect(3.5, 3.5, 6.0, 6.0, 0.75, false);
            g.rect(14.5, 14.5, 6.0, 6.0, 0.75, false);
            g.path("M4 16.5v4h4m-4 0 5-5M16.5 4h4v4m0-4-5 5");
        }
        Icon::AppsGrid => {
            g.rect(3.5, 3.5, 6.5, 6.5, 1.0, false);
            g.rect(14.0, 3.5, 6.5, 6.5, 1.0, false);
            g.rect(3.5, 14.0, 6.5, 6.5, 1.0, false);
            g.rect(14.0, 14.0, 6.5, 6.5, 1.0, false);
        }
        Icon::ArrangeWindows => {
            g.rect(3.5, 3.5, 7.5, 17.0, 1.0, false);
            g.rect(14.0, 3.5, 6.5, 6.5, 1.0, false);
            g.rect(14.0, 14.0, 6.5, 6.5, 1.0, false);
        }
        Icon::GatherWindows => {
            g.path("M7 15H4.5a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1H14a1 1 0 0 1 1 1V7");
            g.rect(9.0, 9.0, 11.5, 11.5, 1.0, false);
        }
        Icon::GestureStroke => {
            g.path("M4.5 15.5 8 6l5 12 4-10h1.5");
            g.circle(4.0, 17.0, 1.0, false);
            g.circle(20.0, 8.0, 1.0, false);
        }
        Icon::Eraser => {
            g.path("m4 14 8-10 8 6.5-7.5 9.5H9Z");
            g.path("m8 9 8 6.5M12.5 20H21");
        }
        Icon::Pin => {
            g.path("M8 3.5h8m-7 0v6L6 13v2h12v-2l-3-3.5v-6M12 15v5.5");
        }
        _ => return false,
    }
    true
}
