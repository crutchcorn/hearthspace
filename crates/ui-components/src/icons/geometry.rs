use super::Icon;
use xilem::masonry::{
    kurbo::{BezPath, Circle, Ellipse, Rect, Shape},
    peniko::Fill,
};

#[path = "actions.rs"]
mod actions;
#[path = "apps.rs"]
mod apps;
#[path = "canvas.rs"]
mod canvas;
#[path = "system.rs"]
mod system;

pub(super) struct Part {
    pub(super) path: BezPath,
    pub(super) fill: Option<Fill>,
}
pub(super) struct Geometry(pub(super) Vec<Part>);

impl Geometry {
    pub(super) fn new(icon: Icon, filled: bool) -> Self {
        let mut geometry = Self(Vec::new());
        if !(actions::append(icon, filled, &mut geometry)
            || apps::append(icon, filled, &mut geometry)
            || canvas::append(icon, filled, &mut geometry)
            || system::append(icon, filled, &mut geometry))
        {
            match icon {
                Icon::Power => {
                    geometry.path("M12 3v9M7 5.5a8.5 8.5 0 1 0 10 0");
                }
                Icon::Lock => {
                    geometry.rect(5.0, 10.0, 14.0, 11.0, 1.0, false);
                    geometry.path("M8 10V7a4 4 0 0 1 8 0v3M12 14v3");
                }
                Icon::LogOut => {
                    geometry.path("M10 3H4v18h6M9 12h12m-5-5 5 5-5 5");
                }
                Icon::User => {
                    geometry.circle(12.0, 7.0, 4.0, false);
                    geometry.path("M4 21v-2a8 8 0 0 1 16 0v2");
                }
                Icon::Eye => {
                    geometry.path("M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z");
                    geometry.circle(12.0, 12.0, 3.0, false);
                }
                Icon::Battery {
                    percentage,
                    charging,
                } => {
                    geometry.rect(3.0, 7.0, 16.0, 10.0, 1.0, false);
                    geometry.path("M21 10v4");
                    if charging {
                        geometry.fill_path("M12 8 8 12.5h3L10 16l5-5h-3Z", false);
                    } else if percentage > 0 {
                        geometry.rect(
                            5.0,
                            9.0,
                            12.0 * f64::from(percentage.min(100)) / 100.0,
                            6.0,
                            0.0,
                            true,
                        );
                    }
                }
                _ => unreachable!("every static icon has source geometry"),
            }
        }
        geometry
    }

    fn push(&mut self, path: BezPath, filled: bool) {
        self.0.push(Part {
            path,
            fill: filled.then_some(Fill::NonZero),
        });
    }

    fn path(&mut self, data: &str) {
        self.push(
            BezPath::from_svg(data).expect("valid bundled icon path"),
            false,
        );
    }

    fn fill_path(&mut self, data: &str, even_odd: bool) {
        self.0.push(Part {
            path: BezPath::from_svg(data).expect("valid bundled icon path"),
            fill: Some(if even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            }),
        });
    }

    fn rect(&mut self, x: f64, y: f64, width: f64, height: f64, radius: f64, filled: bool) {
        self.push(
            Rect::new(x, y, x + width, y + height)
                .to_rounded_rect(radius)
                .to_path(0.01),
            filled,
        );
    }

    fn circle(&mut self, x: f64, y: f64, radius: f64, filled: bool) {
        self.push(Circle::new((x, y), radius).to_path(0.01), filled);
    }

    fn ellipse(&mut self, x: f64, y: f64, rx: f64, ry: f64, rotation: f64, filled: bool) {
        self.push(
            Ellipse::new((x, y), (rx, ry), rotation.to_radians()).to_path(0.01),
            filled,
        );
    }
}
