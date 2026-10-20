//! Exact geometry from hearthspace-ui-experiments/src/icons/system.tsx.
use super::{Geometry, Icon};

pub(super) fn append(icon: Icon, _: bool, g: &mut Geometry) -> bool {
    match icon {
        Icon::BatteryFull => {
            g.rect(3.0, 7.0, 16.0, 10.0, 1.0, false);
            g.path("M21 10v4");
            g.path("M6.5 10v4M11 10v4M15.5 10v4");
        }
        Icon::Bell => {
            g.path("M7 16V9l2-3h6l2 3v7l2 2H5l2-2ZM12 3v3M10 21h4");
        }
        Icon::BellUnread => {
            g.path("M7 16V9l2-3h6l2 3v7l2 2H5l2-2ZM12 3v3M10 21h4");
            g.circle(19.5, 4.5, 1.5, true);
        }
        Icon::Clock => {
            g.circle(12.0, 12.0, 9.0, false);
            g.path("M12 6v6h5");
        }
        Icon::Volume => {
            g.path("M3 9h4l5-4v14l-5-4H3V9Z");
            g.path("M16 8c2.5 2 2.5 6 0 8M18 5c3.5 3.7 3.5 10.3 0 14");
        }
        Icon::VolumeMuted => {
            g.path("M3 9h4l5-4v14l-5-4H3V9Z");
            g.path("m16 9 5 6m0-6-5 6");
        }
        Icon::Wifi => {
            g.path("M3 8c5-4 13-4 18 0M6.5 11.5c3.1-2.5 7.9-2.5 11 0M10 14.5c1.1-.8 2.9-.8 4 0");
            g.circle(12.0, 20.0, 1.0, false);
        }
        Icon::WifiOff => {
            g.path("M3 8c5-4 13-4 18 0M6.5 11.5c3.1-2.5 7.9-2.5 11 0M10 14.5c1.1-.8 2.9-.8 4 0");
            g.circle(12.0, 20.0, 1.0, false);
            g.path("m4 20 16-16");
        }
        Icon::Monitor => {
            g.rect(3.0, 4.0, 18.0, 13.0, 1.0, false);
            g.path("M12 17v4M9 21h6");
        }
        Icon::Laptop => {
            g.rect(5.0, 4.0, 14.0, 12.0, 1.0, false);
            g.path("m5 16-2 4h18l-2-4");
        }
        Icon::Sun => {
            g.circle(12.0, 12.0, 4.0, false);
            g.path("M12 3v2M12 19v2M3 12h2M19 12h2M5.5 5.5 7 7M17 17l1.5 1.5M5.5 18.5 7 17M17 7l1.5-1.5");
        }
        Icon::Moon => {
            g.path("M15 3.5A8.5 8.5 0 1 0 20.5 15A8 8 0 0 1 15 3.5Z");
        }
        Icon::Appearance => {
            g.circle(12.0, 12.0, 8.5, false);
            g.path("M12 3.5v17M12 12h8.5");
        }
        Icon::Processor => {
            g.rect(6.0, 6.0, 12.0, 12.0, 1.0, false);
            g.path("M9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3");
            g.rect(9.0, 10.0, 6.0, 4.0, 0.0, false);
        }
        Icon::Sliders => {
            g.path("M3 5h4m4 0h10M3 12h11m4 0h3M3 19h6m4 0h8");
            g.rect(7.0, 3.0, 4.0, 4.0, 1.0, false);
            g.rect(14.0, 10.0, 4.0, 4.0, 1.0, false);
            g.rect(9.0, 17.0, 4.0, 4.0, 1.0, false);
        }
        Icon::Settings => {
            g.path("M9 3h6v2l2 2h2v2h2v6h-2v2h-2l-2 2v2H9v-2l-2-2H5v-2H3V9h2V7h2l2-2V3Z");
            g.circle(12.0, 12.0, 3.0, false);
        }
        _ => return false,
    }
    true
}
