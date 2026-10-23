//! Exact geometry from hearthspace-ui-experiments/src/icons/apps.tsx.
use super::{Geometry, Icon};

pub(super) fn append(icon: Icon, filled: bool, g: &mut Geometry) -> bool {
    if filled {
        match icon {
            Icon::Compass => {
                g.fill_path(
                    "M12 3.5a8.5 8.5 0 1 1 0 17 8.5 8.5 0 0 1 0-17Zm3.5 4-2 5.5-5 3.5 2-5.5Z",
                    true,
                );
                return true;
            }
            Icon::Folder => {
                g.fill_path("M4.5 4.5h5L12 7h7.5a1 1 0 0 1 1 1v10.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1v-13a1 1 0 0 1 1-1ZM5 9v1.25h14V9Z", true);
                return true;
            }
            Icon::Terminal => {
                g.fill_path("M5 4.5h14A1.5 1.5 0 0 1 20.5 6v12a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18V6A1.5 1.5 0 0 1 5 4.5Zm1.5 4.5 3 3-3 3 1 1 4-4-4-4Zm7 5.75v1.5h4v-1.5Z", true);
                return true;
            }
            Icon::Note => {
                g.fill_path("M4.5 3.5h15a1 1 0 0 1 1 1v8.25h-6a1.75 1.75 0 0 0-1.75 1.75v6H4.5a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1ZM7.5 7.25v1.5h9v-1.5Zm0 3.5v1.5h5v-1.5Z", true);
                g.fill_path("M14.25 14.25h5.5l-5.5 5.5Z", false);
                return true;
            }
            Icon::Music => {
                g.fill_path("M8.25 18V5.5l11.5-2.875V16.5h-1.5V8l-8.5 2.125V18Z", false);
                g.ellipse(6.5, 18.0, 2.5, 2.0, 0.0, true);
                g.ellipse(16.5, 16.5, 2.5, 2.0, 0.0, true);
                return true;
            }
            Icon::Photo => {
                g.fill_path("M5 4.5h14A1.5 1.5 0 0 1 20.5 6v12a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18V6A1.5 1.5 0 0 1 5 4.5Zm11.5 2.5a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3ZM5 15.25V18h14v-1.5h-1.5l-6-6-4.75 4.75Z", true);
                return true;
            }
            Icon::Calendar => {
                g.fill_path("M5.5 5.5h13A1.5 1.5 0 0 1 20 7v12a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 19V7a1.5 1.5 0 0 1 1.5-1.5Zm0 3.75v1.5h13v-1.5ZM7 12.5v2h2v-2Zm4 0v2h2v-2Zm4 0v2h2v-2Zm-8 4v2h2v-2Zm4 0v2h2v-2Zm4 0v2h2v-2Z", true);
                g.path("M8 3.5v4m8-4v4");
                return true;
            }
            Icon::AppSettings => {
                g.path("M8 3.5V7m0 4v9.5M16 3.5V14m0 4v2.5");
                g.rect(6.0, 7.0, 4.0, 4.0, 0.5, true);
                g.rect(14.0, 14.0, 4.0, 4.0, 0.5, true);
                return true;
            }
            Icon::FileText => {
                g.fill_path("M6 3.5h7.25v4A1.75 1.75 0 0 0 15 9.25h4V19.5a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1ZM8 11.75v1.5h8v-1.5Zm0 4v1.5h5v-1.5Z", true);
                g.fill_path("M14.75 4.25v3.25a.25.25 0 0 0 .25.25h3.25Z", false);
                return true;
            }
            Icon::ImagePlus => {
                g.fill_path("M5 4.5h7V10a1 1 0 0 0 1 1h7.5v7a1.5 1.5 0 0 1-1.5 1.5H5A1.5 1.5 0 0 1 3.5 18V6A1.5 1.5 0 0 1 5 4.5ZM5 15.25V18h14v-1.5h-1.5l-6-6-4.75 4.75Z", true);
                g.path("M18 3v5m-2.5-2.5h5");
                return true;
            }
            _ => {}
        }
    }
    match icon {
        Icon::Compass => {
            g.circle(12.0, 12.0, 8.5, false);
            g.path("m15.5 7.5-2 5.5-5 3.5 2-5.5Z");
            g.path("m10.5 11 3 2");
        }
        Icon::Folder => {
            g.path("M3.5 9.5v-4a1 1 0 0 1 1-1h5l2.5 2.5h7.5a1 1 0 0 1 1 1v10.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1Z");
            g.path("M3.5 9.5h17");
        }
        Icon::Terminal => {
            g.rect(3.5, 4.5, 17.0, 15.0, 1.5, false);
            g.path("m7 8.5 3.5 3.5L7 15.5m6.5 0h4");
        }
        Icon::Note => {
            g.path("M4.5 3.5h15a1 1 0 0 1 1 1v9l-7 7h-9a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1Z");
            g.path("M13.5 20.5v-6a1 1 0 0 1 1-1h6M7.5 8h9m-9 3.5h5");
        }
        Icon::Music => {
            g.path("M9 18V6l10-2.5v13M9 9.5 19 7");
            g.ellipse(6.5, 18.0, 2.5, 2.0, 0.0, false);
            g.ellipse(16.5, 16.5, 2.5, 2.0, 0.0, false);
        }
        Icon::Photo => {
            g.rect(3.5, 4.5, 17.0, 15.0, 1.5, false);
            g.path("M3.5 14.5h4l4-4 6 6h3");
            g.circle(16.5, 8.5, 1.5, false);
        }
        Icon::Calendar => {
            g.rect(4.0, 5.5, 16.0, 15.0, 1.5, false);
            g.path("M8 3.5v4m8-4v4M4 10h16");
            g.path("M7.5 13.5h1m3 0h1m3 0h1m-9 4h1m3 0h1m3 0h1");
        }
        Icon::AppSettings => {
            g.path("M8 3.5V7m0 4v9.5M16 3.5V14m0 4v2.5");
            g.rect(6.0, 7.0, 4.0, 4.0, 0.5, false);
            g.rect(14.0, 14.0, 4.0, 4.0, 0.5, false);
        }
        Icon::FileText => {
            g.path("M6 3.5h8l5 5v11a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-15a1 1 0 0 1 1-1Z");
            g.path("M14 3.5v4a1 1 0 0 0 1 1h4M8 12.5h8m-8 4h5");
        }
        Icon::ImagePlus => {
            g.path(
                "M11 4.5H5A1.5 1.5 0 0 0 3.5 6v12A1.5 1.5 0 0 0 5 19.5h14a1.5 1.5 0 0 0 1.5-1.5v-7",
            );
            g.path("M3.5 14.5h4l4-4 6 6h3M18 3v5m-2.5-2.5h5");
        }
        _ => return false,
    }
    true
}
