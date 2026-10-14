//! Shell palettes declare a fixed size through xdg min/max constraints.
use smithay::{
    utils::{Logical, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};
use wayland_server::protocol::wl_surface::WlSurface;

use crate::config::CONTROL_BAR_HEIGHT;

use super::super::{App, ManagedSurface, ManagedWindowKind};

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
        let Some(size) = palette_size(hints.min_size, hints.max_size, output) else {
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
) -> Option<Size<i32, Logical>> {
    // Ordinary application constraints never enter this shell-specific path.
    // Zero means unconstrained under xdg-shell, not a request for zero pixels.
    if min != max || min.w <= 0 || min.h <= 0 {
        return None;
    }
    let width = output.w.max(1);
    let height = output.h.saturating_sub(CONTROL_BAR_HEIGHT).max(1);
    Some((min.w.min(width), min.h.min(height)).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_palettes_follow_changing_content_and_stay_inside_the_output() {
        let output = (1280, 800).into();
        assert_eq!(
            palette_size((360, 52).into(), (360, 52).into(), output),
            Some((360, 52).into())
        );
        assert_eq!(
            palette_size((360, 440).into(), (360, 440).into(), output),
            Some((360, 440).into())
        );
        assert_eq!(
            palette_size((2000, 2000).into(), (2000, 2000).into(), output),
            Some((1280, 800 - CONTROL_BAR_HEIGHT).into())
        );
    }

    #[test]
    fn unconstrained_or_variable_size_hints_do_not_force_a_resize() {
        let output = (1280, 800).into();
        assert_eq!(palette_size((0, 0).into(), (0, 0).into(), output), None);
        assert_eq!(
            palette_size((360, 52).into(), (360, 440).into(), output),
            None
        );
        assert_eq!(palette_size((0, 52).into(), (0, 52).into(), output), None);
    }
}
