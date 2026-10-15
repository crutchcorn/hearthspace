//! Shell palettes request their measured content height through xdg constraints.
//! Search width follows its output; other palettes retain their requested width.
use hearthspace_common::launcher_output_name_from_app_id;
use smithay::{
    utils::{Logical, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::config::CONTROL_BAR_HEIGHT;

use super::super::{App, ManagedSurface, ManagedWindowKind};
use super::window_app_id;

impl App {
    pub(in crate::compositor) fn configure_shell_palette_size(&self, root: &WlSurface) {
        let Some(window) = self.windows.iter().find(|window| {
            window.kind == ManagedWindowKind::Launcher
                && window.surface.wl_surface().as_ref() == Some(root)
        }) else {
            return;
        };
        let surface = match &window.surface {
            ManagedSurface::Xdg(surface) => surface,
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(_) => return,
        };
        let Some(output) =
            self.output_client_logical_size_for_name(window.shell_output_name.as_deref())
        else {
            return;
        };
        let hints = with_states(root, |states| {
            *states.cached_state.get::<SurfaceCachedState>().current()
        });
        let search = window_app_id(&window.surface)
            .as_deref()
            .and_then(launcher_output_name_from_app_id)
            .is_some();
        let Some(size) = palette_size(hints.min_size, hints.max_size, output, search) else {
            return;
        };
        let changed = surface.with_pending_state(|state| {
            if state.size == Some(size) {
                return false;
            }
            state.size = Some(size);
            true
        });
        if changed {
            surface.send_configure();
        }
    }
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
        // Use output-logical units so scale and monitor origin cannot change
        // the fraction. Keep a small inset on narrow outputs as well.
        ((f64::from(width) * 0.6).round() as i32)
            .max(360)
            .min(width.saturating_sub(24).max(1))
    } else {
        min.w.min(width)
    };
    Some((width, min.h.min(height)).into())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn search_tracks_output_width_while_retaining_its_measured_height() {
        for (output, expected) in [
            ((1280, 800), (768, 180)),
            ((1920, 1080), (1152, 180)),
            ((800, 600), (480, 180)),
            ((600, 400), (360, 180)),
            ((320, 200), (296, 152)),
            ((20, 20), (1, 1)),
        ] {
            assert_eq!(
                palette_size((360, 180).into(), (360, 180).into(), output.into(), true),
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
