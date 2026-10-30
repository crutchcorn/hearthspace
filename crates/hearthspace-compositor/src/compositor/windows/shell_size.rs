//! Shell palettes request their measured content height through xdg constraints.
//! Apps List retains its requested width, capped to the output's corner margins.
use hearthspace_common::{
    SHELL_CORNER_CLEARANCE, SHELL_CORNER_MARGIN, canvas_map_output_name_from_app_id,
    launcher_output_name_from_app_id, power_confirm_output_name_from_app_id,
    shell_corner_from_app_id,
};
use smithay::{
    utils::{Logical, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::config::CONTROL_BAR_HEIGHT;

use super::super::{App, ManagedWindowKind};
use super::window_app_id;

impl App {
    pub(in crate::compositor) fn configure_shell_palette_size(&self, root: &WlSurface) {
        let Some(window) = self.windows.iter().find(|window| {
            matches!(
                window.kind,
                ManagedWindowKind::ShellBar
                    | ManagedWindowKind::Launcher
                    | ManagedWindowKind::ControlOsd
                    | ManagedWindowKind::DesktopItem
            ) && window.surface.wl_surface().as_ref() == Some(root)
        }) else {
            return;
        };
        let Some(surface) = window.surface.xdg_toplevel() else {
            return;
        };
        let Some(output) =
            self.output_client_logical_size_for_name(window.shell_output_name.as_deref())
        else {
            return;
        };
        let app_id = window_app_id(&window.surface);
        let corner = app_id.as_deref().and_then(shell_corner_from_app_id);
        if window.kind == ManagedWindowKind::ShellBar
            && corner.is_none()
            && !app_id.as_deref().is_some_and(super::is_frame_shell_app_id)
        {
            return;
        }
        let Some(requested) = fixed_size(root) else {
            return;
        };
        let search = app_id
            .as_deref()
            .and_then(launcher_output_name_from_app_id)
            .is_some();
        let power_confirmation = app_id
            .as_deref()
            .and_then(power_confirm_output_name_from_app_id)
            .is_some();
        let canvas_map = app_id
            .as_deref()
            .and_then(canvas_map_output_name_from_app_id)
            .is_some();
        let Some(mut size) = power_confirmation
            .then(|| super::power_confirmation::power_confirmation_size(requested, output))
            .or_else(|| {
                corner
                    .map(|corner| {
                        super::shell_corners::corner_size(corner, Some(requested), output)
                    })
                    .or_else(|| palette_size(requested, requested, output, search))
            })
        else {
            return;
        };
        if canvas_map {
            size.w = map_width(requested.w, output.w);
        }
        if !power_confirmation
            && corner.is_none()
            && !matches!(
                window.kind,
                ManagedWindowKind::ControlOsd | ManagedWindowKind::DesktopItem
            )
            && self
                .output_logical_rect_for_name(window.shell_output_name.as_deref())
                .is_some_and(|rect| self.has_shell_corners(rect))
        {
            // Keep the popup's own controls exposed. On short outputs it may
            // cover the opposite corner so its header cannot consume all of
            // the scrollable content area.
            size.h = size
                .h
                .min(output.h.saturating_sub(SHELL_CORNER_CLEARANCE).max(1));
        }
        let changed = surface.with_pending_state(|state| {
            if state.size == Some(size) {
                return false;
            }
            state.size = Some(size);
            if window.kind == ManagedWindowKind::DesktopItem {
                // The same native surface changes from a canvas tile to a
                // compact frame button, and back again when it is unpinned.
                state.bounds = Some(size);
            }
            true
        });
        if changed {
            crate::compositor::gtk_shell::send_configure(surface);
        }
    }
}

fn map_width(requested: i32, output: i32) -> i32 {
    // Leave the Overview's twelve-pixel anchored margin visible even when its
    // preferred width does not fit. Keep narrow/short content scrollable.
    requested.min(output.saturating_sub(28).max(1)).max(1)
}

pub(super) fn fixed_size(root: &WlSurface) -> Option<Size<i32, Logical>> {
    let hints = with_states(root, |states| {
        *states.cached_state.get::<SurfaceCachedState>().current()
    });
    (hints.min_size == hints.max_size && hints.min_size.w > 0 && hints.min_size.h > 0)
        .then_some(hints.min_size)
}

fn palette_size(
    min: Size<i32, Logical>,
    max: Size<i32, Logical>,
    output: Size<i32, Logical>,
    search: bool,
) -> Option<Size<i32, Logical>> {
    // Ordinary application constraints never enter this shell-specific path.
    // Zero means unconstrained under xdg-shell, not a request for zero pixels.
    if min != max || min.w <= 0 || min.h <= 0 {
        return None;
    }
    let width = output.w.max(1);
    let height = output.h.saturating_sub(CONTROL_BAR_HEIGHT).max(1);
    let width = if search {
        min.w
            .min(width.saturating_sub(2 * SHELL_CORNER_MARGIN).max(1))
    } else {
        min.w.min(width)
    };
    Some((width, min.h.min(height)).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_keeps_its_margin_on_narrow_and_scaled_outputs() {
        for (output, expected) in [(1280, 352), (640, 352), (375, 347), (320, 292), (20, 1)] {
            assert_eq!(map_width(352, output), expected);
        }
    }

    #[test]
    fn fixed_palettes_follow_changing_content_and_stay_inside_the_output() {
        let output = (1280, 800).into();
        assert_eq!(
            palette_size((360, 52).into(), (360, 52).into(), output, false),
            Some((360, 52).into())
        );
        assert_eq!(
            palette_size((360, 440).into(), (360, 440).into(), output, false),
            Some((360, 440).into())
        );
        assert_eq!(
            palette_size((2000, 2000).into(), (2000, 2000).into(), output, false),
            Some((1280, 800 - CONTROL_BAR_HEIGHT).into())
        );
    }

    #[test]
    fn unconstrained_or_variable_size_hints_do_not_force_a_resize() {
        let output = (1280, 800).into();
        assert_eq!(
            palette_size((0, 0).into(), (0, 0).into(), output, false),
            None
        );
        assert_eq!(
            palette_size((360, 52).into(), (360, 440).into(), output, false),
            None
        );
        assert_eq!(
            palette_size((0, 52).into(), (0, 52).into(), output, false),
            None
        );
    }

    #[test]
    fn search_caps_its_requested_width_while_retaining_its_measured_height() {
        for (output, expected) in [
            ((1280, 800), (650, 180)),
            ((1920, 1080), (650, 180)),
            ((800, 600), (650, 180)),
            ((600, 400), (552, 180)),
            ((320, 200), (272, 146)),
            ((20, 20), (1, 1)),
        ] {
            assert_eq!(
                palette_size((650, 180).into(), (650, 180).into(), output.into(), true),
                Some(expected.into()),
            );
        }
    }

    #[test]
    fn search_waits_for_measured_content_hints() {
        let output = (1280, 800).into();
        assert_eq!(
            palette_size((0, 0).into(), (0, 0).into(), output, true),
            None
        );
        assert_eq!(
            palette_size((360, 52).into(), (768, 440).into(), output, true),
            None
        );
    }
}
