//! Optional output-space placement for EWMH overlays.
use super::*;
use protocol::xwayland_satellite_overlay_v1::XwaylandSatelliteOverlayV1;
pub(super) mod protocol {
    #![allow(non_camel_case_types, non_upper_case_globals, unused_imports)]
    use wayland_client::{self, protocol::*};
    use wayland_protocols::xdg::shell::client::*;
    mod __interfaces {
        use wayland_client::backend as wayland_backend;
        use wayland_client::protocol::__interfaces::*;
        use wayland_protocols::xdg::shell::client::__interfaces::*;
        wayland_scanner::generate_interfaces!("protocols/xwayland-satellite-overlay-v1.xml");
    }
    use self::__interfaces::*;
    wayland_scanner::generate_client_code!("protocols/xwayland-satellite-overlay-v1.xml");
}

wayland_client::delegate_noop!(MyWorld: XwaylandSatelliteOverlayV1);

struct OutputArea {
    entity: Entity,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

impl<S: X11Selection> InnerServerState<S> {
    pub fn overlay_role(&self, normalized_x: f64, normalized_y: f64) -> WindowRole {
        let outputs: Vec<_> = self
            .world
            .query::<&OutputDimensions>()
            .iter()
            .filter(|(_, d)| d.width > 0 && d.height > 0)
            .map(|(entity, d)| OutputArea {
                entity,
                x: f64::from(d.x),
                y: f64::from(d.y),
                width: f64::from(d.width),
                height: f64::from(d.height),
            })
            .collect();
        select_output(&outputs, normalized_x, normalized_y)
            .map_or(WindowRole::Overlay(3, None), |(corner, entity)| {
                WindowRole::Overlay(corner, Some(entity))
            })
    }
}

fn select_output(
    outputs: &[OutputArea],
    normalized_x: f64,
    normalized_y: f64,
) -> Option<(u32, Entity)> {
    // Satellite exposes a translated/scaled X root. Normalize within its
    // bounding rectangle, then recover the host output before choosing a corner.
    let left = outputs.iter().map(|o| o.x).fold(f64::INFINITY, f64::min);
    let top = outputs.iter().map(|o| o.y).fold(f64::INFINITY, f64::min);
    let right = outputs
        .iter()
        .map(|o| o.x + o.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let bottom = outputs
        .iter()
        .map(|o| o.y + o.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let x = left + normalized_x * (right - left);
    let y = top + normalized_y * (bottom - top);
    let distance = |o: &OutputArea| {
        (x - x.clamp(o.x, o.x + o.width)).powi(2) + (y - y.clamp(o.y, o.y + o.height)).powi(2)
    };
    let output = outputs.iter().min_by(|a, b| {
        distance(a)
            .total_cmp(&distance(b))
            .then_with(|| a.entity.to_bits().cmp(&b.entity.to_bits()))
    })?;
    let corner = u32::from(x >= output.x + output.width / 2.)
        | (u32::from(y >= output.y + output.height / 2.) << 1);
    Some((corner, output.entity))
}

#[cfg(test)]
mod tests {
    use super::*;
    use hecs::World;
    #[test]
    fn translated_output_layout_retains_requested_output_and_corner() {
        let mut world = World::new();
        let a = world.spawn(());
        let b = world.spawn(());
        let outputs = [
            OutputArea {
                entity: a,
                x: -800.,
                y: 0.,
                width: 800.,
                height: 600.,
            },
            OutputArea {
                entity: b,
                x: 0.,
                y: 0.,
                width: 800.,
                height: 600.,
            },
        ];
        assert_eq!(select_output(&outputs, 0.875, 0.75), Some((3, b)));
        assert_eq!(select_output(&outputs, 0.125, 0.25), Some((0, a)));
    }
    #[test]
    fn requests_in_monitor_gaps_and_beyond_root_choose_nearest_output() {
        let mut world = World::new();
        let a = world.spawn(());
        let b = world.spawn(());
        let outputs = [
            OutputArea {
                entity: a,
                x: 0.,
                y: 0.,
                width: 800.,
                height: 600.,
            },
            OutputArea {
                entity: b,
                x: 1200.,
                y: 200.,
                width: 800.,
                height: 600.,
            },
        ];
        assert_eq!(select_output(&outputs, 0.55, 0.5), Some((0, b)));
        assert_eq!(select_output(&outputs, 1.1, 1.1), Some((3, b)));
        assert_eq!(select_output(&[], 0.5, 0.5), None);
    }
}
