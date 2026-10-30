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
            g.path("M9.56 5.53Q9.95 5.31 10.05 4.87L10.34 3.58Q10.44 3.14 10.89 3.14L13.11 3.14Q13.56 3.14 13.66 3.58L13.95 4.87Q14.05 5.31 14.44 5.53L16.38 6.66Q16.77 6.88 17.2 6.75L18.47 6.35Q18.89 6.21 19.12 6.6L20.23 8.53Q20.46 8.92 20.13 9.23L19.15 10.12Q18.82 10.43 18.82 10.88L18.82 13.12Q18.82 13.57 19.15 13.88L20.13 14.77Q20.46 15.08 20.23 15.47L19.12 17.4Q18.89 17.79 18.47 17.65L17.2 17.25Q16.77 17.12 16.38 17.34L14.44 18.47Q14.05 18.69 13.95 19.13L13.66 20.42Q13.56 20.86 13.11 20.86L10.89 20.86Q10.44 20.86 10.34 20.42L10.05 19.13Q9.95 18.69 9.56 18.47L7.62 17.34Q7.23 17.12 6.8 17.25L5.53 17.65Q5.11 17.79 4.88 17.4L3.77 15.47Q3.54 15.08 3.87 14.77L4.85 13.88Q5.18 13.57 5.18 13.12L5.18 10.88Q5.18 10.43 4.85 10.12L3.87 9.23Q3.54 8.92 3.77 8.53L4.88 6.6Q5.11 6.21 5.53 6.35L6.8 6.75Q7.23 6.88 7.62 6.66Z");
            g.circle(12.0, 12.0, 3.0, false);
        }
        _ => return false,
    }
    true
}
