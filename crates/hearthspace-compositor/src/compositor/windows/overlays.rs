//! Output-space placement for unmanaged overlays, including live frame clearance.
use crate::compositor::{App, ManagedWindowKind};
use smithay::{
    utils::{Logical, Point, Rectangle, Size},
    wayland::{compositor::with_states, shell::xdg::ToplevelSurface},
};
const GAP: i32 = hearthspace_common::SHELL_CORNER_MARGIN;
#[derive(Clone)]
pub(in crate::compositor) struct Hint {
    pub toplevel: wayland_server::backend::ObjectId,
    pub corner: u32,
    pub output: Option<String>,
}

pub(in crate::compositor) struct Hints(pub std::sync::Mutex<Hint>);

pub(in crate::compositor) fn hint_for_toplevel(surface: &ToplevelSurface) -> Option<Hint> {
    use wayland_server::Resource;
    with_states(surface.wl_surface(), |state| {
        let hint = state.data_map.get::<Hints>()?.0.lock().unwrap();
        (hint.toplevel == surface.xdg_toplevel().id()).then(|| hint.clone())
    })
}

impl App {
    pub(in crate::compositor) fn x11_overlay_output_name(&self, index: usize) -> Option<String> {
        self.windows[index]
            .surface
            .xdg_toplevel()
            .and_then(hint_for_toplevel)
            .and_then(|h| h.output)
    }

    pub(in crate::compositor) fn x11_overlays_status(&self) -> Vec<serde_json::Value> {
        let mut rows = Vec::new();
        for (index, window) in self.windows.iter().enumerate() {
            if !window.mapped || window.kind != ManagedWindowKind::X11Overlay {
                continue;
            }
            let rect = self.x11_overlay_rect(index);
            rows.push(serde_json::json!({
                "id": window.id, "x": rect.loc.x, "y": rect.loc.y,
                "width": rect.size.w, "height": rect.size.h,
            }));
        }
        rows
    }

    pub(in crate::compositor) fn x11_overlay_rect(&self, target: usize) -> Rectangle<i32, Logical> {
        let assigned_output = |index: usize| {
            let name = self.x11_overlay_output_name(index);
            self.output_logical_rect_for_name(name.as_deref())
                .unwrap_or_else(|| self.primary_output_logical_rect())
        };
        let output = assigned_output(target);
        let mut blockers: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                if !window.mapped || window.kind != ManagedWindowKind::ShellBar {
                    return None;
                }
                self.layout_glass_rect(index).or_else(|| {
                    let origin = self.shell_bar_logical_origin(index)?
                        + self.window_surface_geometry_loc(index);
                    Some(Rectangle::new(origin, self.window_geometry_size(index)))
                })
            })
            .filter(|rect| rect.overlaps(output))
            .collect();
        // Stable window identities keep the stack independent of focus/restacking.
        let mut overlays: Vec<_> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(index, window)| {
                window.kind == ManagedWindowKind::X11Overlay
                    && assigned_output(*index) == output
                    && (window.mapped || *index == target)
            })
            .collect();
        overlays.sort_by_key(|(_, window)| window.id);
        for (index, window) in overlays {
            let corner = window
                .surface
                .xdg_toplevel()
                .and_then(hint_for_toplevel)
                .map_or(3, |hint| hint.corner);
            let rect = place(output, self.window_geometry_size(index), corner, &blockers);
            if index == target {
                return rect;
            }
            blockers.push(rect);
        }
        Rectangle::new(output.loc, self.window_geometry_size(target))
    }
}
fn place(
    output: Rectangle<i32, Logical>,
    size: Size<i32, Logical>,
    corner: u32,
    blockers: &[Rectangle<i32, Logical>],
) -> Rectangle<i32, Logical> {
    // Reduce edge padding only when the output cannot fit the normal margin.
    // Oversized clients retain their buffer size and start at the output edge.
    let padding_x = GAP.min((output.size.w - size.w).max(0) / 2);
    let padding_y = GAP.min((output.size.h - size.h).max(0) / 2);
    let min = output.loc + Point::from((padding_x, padding_y));
    let max = output.loc
        + Point::from((
            (output.size.w - size.w - padding_x).max(padding_x),
            (output.size.h - size.h - padding_y).max(padding_y),
        ));
    let ideal = Point::from((
        if corner & 1 != 0 { max.x } else { min.x },
        if corner & 2 != 0 { max.y } else { min.y },
    ));
    let mut xs = vec![ideal.x];
    let mut ys = vec![ideal.y];
    for b in blockers {
        xs.extend([b.loc.x - size.w - GAP, b.loc.x + b.size.w + GAP]);
        ys.extend([b.loc.y - size.h - GAP, b.loc.y + b.size.h + GAP]);
    }
    let mut best = None;
    for x in xs {
        for &y in &ys {
            if x < min.x || x > max.x || y < min.y || y > max.y {
                continue;
            }
            let rect = Rectangle::new((x, y).into(), size);
            if blockers.iter().any(|b| {
                rect.overlaps(Rectangle::new(
                    b.loc - Point::from((GAP, GAP)),
                    (b.size.w + 2 * GAP, b.size.h + 2 * GAP).into(),
                ))
            }) {
                continue;
            }
            let distance = i64::from(x - ideal.x).pow(2) + i64::from(y - ideal.y).pow(2);
            if best.as_ref().is_none_or(|(score, _)| distance < *score) {
                best = Some((distance, rect));
            }
        }
    }
    // If every candidate is blocked, keep the requested corner rather than
    // moving the client offscreen or resizing its contents.
    best.map_or_else(|| Rectangle::new(ideal, size), |(_, rect)| rect)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bottom_right_overlay_avoids_frame_and_other_toasts() {
        let output = Rectangle::new((1280, 0).into(), (1280, 800).into());
        let frame = Rectangle::new((2240, 720).into(), (300, 60).into());
        let first = place(output, (280, 100).into(), 3, &[frame]);
        assert!(!first.overlaps(frame));
        assert_eq!(first.loc.x, 2256);
        assert_eq!(first.loc.y, 596);
        let second = place(output, (280, 100).into(), 3, &[frame, first]);
        assert!(!second.overlaps(first));
        assert!(!second.overlaps(frame));
    }
    #[test]
    fn top_left_overlay_keeps_its_requested_edge() {
        let output = Rectangle::new((-1280, 100).into(), (1280, 800).into());
        assert_eq!(
            place(output, (280, 100).into(), 0, &[]).loc,
            (-1256, 124).into()
        );
    }
    #[test]
    fn small_output_reduces_margins_without_hiding_a_fitting_overlay() {
        let output = Rectangle::new((300, 200).into(), (200, 120).into());
        let rect = place(output, (200, 100).into(), 3, &[]);
        assert_eq!(rect.loc, (300, 210).into());
        assert!(output.contains_rect(rect));
    }
}
