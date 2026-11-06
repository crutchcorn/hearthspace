//! Client size constraints shared by free and grid-snapped interactive resizing.
use smithay::{
    utils::{Logical, Size},
    wayland::{compositor::with_states, shell::xdg::SurfaceCachedState},
};

use crate::{
    compositor::ManagedSurface,
    config::{MIN_WINDOW_HEIGHT, MIN_WINDOW_WIDTH},
};

#[derive(Clone, Copy, Debug)]
pub(super) struct ResizeLimits {
    pub min: Size<i32, Logical>,
    pub max: Size<i32, Logical>,
}

impl ResizeLimits {
    pub(super) fn for_surface(surface: &ManagedSurface) -> Self {
        match surface {
            ManagedSurface::Xdg(surface) => {
                let state = with_states(surface.wl_surface(), |states| {
                    *states.cached_state.get::<SurfaceCachedState>().current()
                });
                Self::new(state.min_size, state.max_size)
            }
        }
    }

    fn new(min: Size<i32, Logical>, max: Size<i32, Logical>) -> Self {
        let (min_w, max_w) = axis_limits(min.w, max.w, MIN_WINDOW_WIDTH);
        let (min_h, max_h) = axis_limits(min.h, max.h, MIN_WINDOW_HEIGHT);
        Self {
            min: (min_w, min_h).into(),
            max: (max_w, max_h).into(),
        }
    }

    pub(super) fn constrain(self, size: Size<i32, Logical>) -> Size<i32, Logical> {
        (
            size.w.clamp(self.min.w, self.max.w),
            size.h.clamp(self.min.h, self.max.h),
        )
            .into()
    }
}

fn axis_limits(min: i32, max: i32, fallback: i32) -> (i32, i32) {
    let max = if max > 0 { max } else { i32::MAX };
    let min = if min > 0 { min } else { fallback };
    // A small fixed-size application must remain smaller than the compositor's
    // default minimum. Prefer its maximum if the client sends conflicting hints.
    (min.min(max), max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(min: (i32, i32), max: (i32, i32)) -> ResizeLimits {
        ResizeLimits::new(min.into(), max.into())
    }

    #[test]
    fn unconstrained_axes_keep_default_minima() {
        let limits = limits((0, 0), (0, 0));
        assert_eq!(limits.constrain((12, 20).into()), (260, 120).into());
        assert_eq!(limits.constrain((900, 650).into()), (900, 650).into());
    }

    #[test]
    fn small_application_bounds_override_default_minima() {
        let limits = limits((0, 40), (180, 75));
        assert_eq!(limits.constrain((500, 500).into()), (180, 75).into());
        assert_eq!(limits.constrain((10, 10).into()), (180, 40).into());
    }

    #[test]
    fn conflicting_hints_preserve_maximum_and_stay_positive() {
        let limits = limits((400, 0), (180, 1));
        assert_eq!(limits.constrain((0, 900).into()), (180, 1).into());
        assert_eq!(axis_limits(-12, -5, 120), (120, i32::MAX));
    }
}
