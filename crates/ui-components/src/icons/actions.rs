//! Exact geometry from hearthspace-ui-experiments/src/icons/actions.tsx.
use super::{Geometry, Icon};

pub(super) fn append(icon: Icon, _: bool, g: &mut Geometry) -> bool {
    match icon {
        Icon::ArrowLeft => {
            g.path("M20 12H4m6-6-6 6 6 6");
        }
        Icon::ArrowUpRight => {
            g.path("M5 19 19 5M9 5h10v10");
        }
        Icon::ArrowDownRight => {
            g.path("m6 6 13 13M9 19h10V9");
        }
        Icon::ChevronLeft => {
            g.path("m15 5-7 7 7 7");
        }
        Icon::ChevronRight => {
            g.path("m9 5 7 7-7 7");
        }
        Icon::ChevronUp => {
            g.path("m5 15 7-7 7 7");
        }
        Icon::ChevronDown => {
            g.path("m5 9 7 7 7-7");
        }
        Icon::Check => {
            g.path("m5 12 4.5 4.5L19 7");
        }
        Icon::CheckCheck => {
            g.path("m3 12 4.5 4.5L17 7m-5 7 2.5 2.5L22 9");
        }
        Icon::Plus => {
            g.path("M5 12h14M12 5v14");
        }
        Icon::X => {
            g.path("m6 6 12 12M6 18 18 6");
        }
        Icon::CornerDownLeft => {
            g.path("M20 5v9H4m5-5-5 5 5 5");
        }
        Icon::GripVertical => {
            g.path("M9 4v3m6-3v3m-6 4v3m6-3v3m-6 4v3m6-3v3");
        }
        Icon::Grip => {
            g.path("M4 9h3m4 0h3m4 0h3M4 15h3m4 0h3m4 0h3");
        }
        Icon::List => {
            g.path("M4 5.5h2m4 0h10M4 12h2m4 0h10M4 18.5h2m4 0h10");
        }
        Icon::Hourglass => {
            g.path("M6 3.5h12m-12 17h12M7 3.5v4l5 4.5-5 4.5v4m10-17v4L12 12l5 4.5v4");
            g.path("M10 6.5h4");
        }
        Icon::Maximize => {
            g.path("M3.5 9v-5.5H9m6 0h5.5V9m0 6v5.5H15m-6 0H3.5V15");
        }
        Icon::Minimize => {
            g.path("M3.5 9H9V3.5m6 0V9h5.5m0 6H15v5.5m-6 0V15H3.5");
        }
        Icon::PanelLeft => {
            g.rect(3.5, 4.5, 17.0, 15.0, 1.0, false);
            g.path("M9 4.5v15m-3-10v5");
        }
        Icon::Play => {
            g.path("M7 4.5 20 12 7 19.5Z");
        }
        Icon::RotateCcw => {
            g.path("M4.5 9a8 8 0 1 1-.3 6M4.5 3.5V9H10");
        }
        Icon::Square => {
            g.rect(5.0, 5.0, 14.0, 14.0, 1.0, false);
        }
        Icon::Trash2 => {
            g.path("M3.5 6.5h17M8.5 6.5v-3h7v3M5.5 6.5l1 14h11l1-14M10 10.5v6m4-6v6");
        }
        Icon::ZoomIn => {
            g.circle(9.5, 9.5, 6.0, false);
            g.path("m14 14 6.5 6.5");
            g.path("M6.5 9.5h6m-3-3v6");
        }
        Icon::ZoomOut => {
            g.circle(9.5, 9.5, 6.0, false);
            g.path("m14 14 6.5 6.5");
            g.path("M6.5 9.5h6");
        }
        _ => return false,
    }
    true
}
