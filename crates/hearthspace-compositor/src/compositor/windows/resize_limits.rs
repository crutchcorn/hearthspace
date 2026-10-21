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
    base: Size<i32, Logical>,
    increment: Size<i32, Logical>,
}

impl ResizeLimits {
    pub(super) fn for_surface(surface: &ManagedSurface) -> Self {
        match surface {
            ManagedSurface::Xdg(surface) => {
                let state = with_states(surface.wl_surface(), |states| {
                    *states.cached_state.get::<SurfaceCachedState>().current()
                });
                Self::new(state.min_size, state.max_size, (0, 0).into(), (1, 1).into())
            }
            #[cfg(feature = "xwayland")]
            ManagedSurface::X11(surface) => {
                // XWM's client scale is fixed at 1.0, so raw ICCCM hints already
                // use logical units. Sanitize before Size rejects negative axes.
                let hints = surface.size_hints().unwrap_or_default();
                Self::from_raw_hints(
                    hints.min_size.unwrap_or_default(),
                    hints.max_size.unwrap_or_default(),
                    hints.base_size.or(hints.min_size).unwrap_or_default(),
                    hints.size_increment.unwrap_or((1, 1)),
                )
            }
        }
    }

    #[cfg(any(feature = "xwayland", test))]
    fn from_raw_hints(
        min: (i32, i32),
        max: (i32, i32),
        base: (i32, i32),
        increment: (i32, i32),
    ) -> Self {
        let size = |(width, height): (i32, i32)| (width.max(0), height.max(0)).into();
        Self::new(
            size(min),
            size(max),
            size(base),
            (increment.0.max(1), increment.1.max(1)).into(),
        )
    }

    fn new(
        min: Size<i32, Logical>,
        max: Size<i32, Logical>,
        base: Size<i32, Logical>,
        increment: Size<i32, Logical>,
    ) -> Self {
        let (min_w, max_w) = axis_limits(min.w, max.w, MIN_WINDOW_WIDTH);
        let (min_h, max_h) = axis_limits(min.h, max.h, MIN_WINDOW_HEIGHT);
        Self {
            min: (min_w, min_h).into(),
            max: (max_w, max_h).into(),
            base: (base.w.max(0), base.h.max(0)).into(),
            increment: (increment.w.max(1), increment.h.max(1)).into(),
        }
    }

    /// Keep terminal cell increments even when the proposed canvas dot is not
    /// a legal client size. Invalid or contradictory hints never escape bounds.
    pub(super) fn constrain(self, size: Size<i32, Logical>) -> Size<i32, Logical> {
        (
            constrain_axis(
                size.w,
                self.min.w,
                self.max.w,
                self.base.w,
                self.increment.w,
            ),
            constrain_axis(
                size.h,
                self.min.h,
                self.max.h,
                self.base.h,
                self.increment.h,
            ),
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

fn constrain_axis(value: i32, min: i32, max: i32, base: i32, increment: i32) -> i32 {
    let value = i64::from(value.clamp(min, max));
    let (min, max, base, step) = (
        i64::from(min),
        i64::from(max),
        i64::from(base),
        i64::from(increment),
    );
    let first = (min - base + step - 1).div_euclid(step).max(0);
    let last = (max - base).div_euclid(step);
    if first > last {
        // No size satisfies both the increment and bounds; bounds win.
        return value as i32;
    }
    let nearest = ((value - base).max(0) + step / 2) / step;
    (base + nearest.clamp(first, last) * step) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(min: (i32, i32), max: (i32, i32)) -> ResizeLimits {
        ResizeLimits::new(min.into(), max.into(), (0, 0).into(), (1, 1).into())
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

    #[test]
    fn terminal_cells_round_inside_bounds_using_the_base_size() {
        let limits = ResizeLimits::new(
            (260, 120).into(),
            (400, 300).into(),
            (4, 8).into(),
            (9, 17).into(),
        );
        assert_eq!(limits.constrain((266, 151).into()), (265, 144).into());
        assert_eq!(limits.constrain((0, 0).into()), (265, 127).into());
        assert_eq!(limits.constrain((500, 500).into()), (400, 297).into());
    }

    #[test]
    fn impossible_increment_interval_keeps_the_client_bounds() {
        let limits = ResizeLimits::new(
            (280, 120).into(),
            (283, 150).into(),
            (0, 0).into(),
            (16, 10).into(),
        );
        assert_eq!(limits.constrain((300, 137).into()), (283, 140).into());
    }

    #[test]
    fn large_or_invalid_increment_hints_do_not_overflow() {
        let limits = ResizeLimits::from_raw_hints((0, 0), (0, 0), (0, -5), (i32::MAX, 0));
        assert_eq!(
            limits.constrain((i32::MAX, 321).into()),
            (i32::MAX, 321).into()
        );
    }

    #[test]
    fn invalid_x11_hints_are_sanitized_before_creating_sizes() {
        let limits = ResizeLimits::from_raw_hints((-12, 0), (180, -5), (-4, -5), (-9, 0));
        assert_eq!(limits.constrain((400, 100).into()), (180, 120).into());
    }
}
