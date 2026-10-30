//! Client pacing follows each completed output, independently of redraw scheduling.
use smithay::{
    desktop::PopupManager,
    utils::{Logical, Rectangle},
    wayland::{
        compositor::{SurfaceAttributes, TraversalAction, with_surface_tree_downward},
        session_lock::LockSurface,
    },
};
use wayland_server::protocol::wl_surface::WlSurface;

use super::{App, CalloopData, ManagedWindowKind, session_lock::send_lock_frame_callbacks};

impl CalloopData {
    // Nested/headless backends complete all outputs together.
    pub(in crate::compositor) fn send_frame_callbacks(
        &mut self,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.state
            .dispatch_frame_callbacks(None, self.start_time.elapsed().as_millis() as u32);
        self.display.flush_clients()?;
        Ok(())
    }

    #[cfg(feature = "udev")]
    pub(in crate::compositor) fn send_output_frame_callbacks(
        &mut self,
        output: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let elapsed = self.start_time.elapsed();
        let callbacks = self
            .state
            .dispatch_frame_callbacks(Some(output), elapsed.as_millis() as u32);
        self.display.flush_clients()?;
        // Count emitted protocol events, including empty batches; this is not
        // an acknowledgement that a client has processed the callbacks.
        tracing::trace!(
            output,
            elapsed_us = elapsed.as_micros() as u64,
            locked = self.state.session_locked(),
            callbacks,
            "native frame callback batch"
        );
        Ok(())
    }
}

impl App {
    fn dispatch_frame_callbacks(&self, output: Option<&str>, time: u32) -> usize {
        let outputs = self.callback_outputs();
        if output.is_some_and(|name| !outputs.iter().any(|(active, _)| *active == name)) {
            return 0;
        }
        let windows = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                let root = window.surface.wl_surface()?;
                let owner = output.and_then(|_| {
                    callback_output(
                        &outputs,
                        self.frame_callback_bounds(index),
                        |rect| self.window_visible_on_output(index, rect),
                        window.shell_output_name.as_deref(),
                    )
                });
                Some((root, owner))
            });
        dispatch_callbacks(windows, self.lock_frame_callback_surfaces(), output, time)
    }

    /// Follow collect_render_elements' content placement and final layout crop.
    /// Shell surfaces have an output-relative origin independent of their saved
    /// canvas position. A fully cropped tree still receives fallback callbacks.
    fn frame_callback_bounds(&self, index: usize) -> Option<Rectangle<i32, Logical>> {
        let projection = self.window_projection(index);
        let bounds = match self.windows[index].kind {
            ManagedWindowKind::Normal | ManagedWindowKind::DesktopItem => projection.bounds,
            ManagedWindowKind::ShellWatcher => return None,
            kind => {
                let origin = if kind == ManagedWindowKind::ShellBar {
                    self.shell_bar_logical_origin(index)?
                } else {
                    self.launcher_logical_origin(index)?
                };
                let scale = self
                    .layout_backing_scale(index)
                    .unwrap_or(projection.scale.into());
                Rectangle::new(
                    origin.to_f64()
                        + self
                            .window_surface_geometry_loc(index)
                            .to_f64()
                            .upscale(scale),
                    self.window_geometry_size(index).to_f64().upscale(scale),
                )
                .to_i32_round()
            }
        };
        self.layout_window_clip(index)
            .map_or(Some(bounds), |clip| bounds.intersection(clip))
    }
}

/// Pick one cadence for a complete window tree, including its popups. Output
/// order is primary first, making equal-area ties stable. Hidden/unmapped
/// clients retain service on their assigned shell output or the primary.
fn callback_output<'a>(
    outputs: &[(&'a str, Rectangle<i32, Logical>)],
    bounds: Option<Rectangle<i32, Logical>>,
    visible: impl Fn(Rectangle<i32, Logical>) -> bool,
    fallback: Option<&str>,
) -> Option<&'a str> {
    let mut best = None;
    let mut best_area = 0;
    for &(name, rect) in outputs {
        if !visible(rect) {
            continue;
        }
        if let Some(overlap) = bounds.and_then(|bounds| bounds.intersection(rect)) {
            let area = i64::from(overlap.size.w) * i64::from(overlap.size.h);
            if area > best_area {
                best = Some(name);
                best_area = area;
            }
        }
    }
    best.or_else(|| {
        outputs
            .iter()
            .find(|(name, _)| Some(*name) == fallback)
            .or_else(|| outputs.first())
            .map(|(name, _)| *name)
    })
}

// The real Wayland regressions use this same selection/dispatch boundary.
// KMS acknowledgement and aggregate redraw policy stay with the native backend.
fn dispatch_callbacks<'a>(
    windows: impl IntoIterator<Item = (WlSurface, Option<&'a str>)>,
    lock_surfaces: Option<&[(String, LockSurface)]>,
    output: Option<&str>,
    time: u32,
) -> usize {
    if let Some(surfaces) = lock_surfaces {
        return send_lock_frame_callbacks(surfaces, output, time);
    }
    let mut callbacks = 0;
    for (root, owner) in windows {
        if output.is_some() && output != owner {
            continue;
        }
        callbacks += send_frames_surface_tree(&root, time);
        // Popups are separate surface trees. GTK menus need these callbacks
        // even when their parent has no new frame callback pending.
        for (popup, _) in PopupManager::popups_for_surface(&root) {
            callbacks += send_frames_surface_tree(popup.wl_surface(), time);
        }
    }
    callbacks
}

pub(in crate::compositor) fn send_frames_surface_tree(surface: &WlSurface, time: u32) -> usize {
    let mut callbacks = 0;
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
                callbacks += 1;
            }
        },
        |_, _, &()| true,
    );
    callbacks
}

#[cfg(test)]
mod geometry_tests;
#[cfg(test)]
pub(in crate::compositor) mod test_support;
#[cfg(test)]
mod tests;
