//! Input capture exists only for an explicitly armed area of focused shell search.
use std::{os::unix::net::UnixDatagram, time::Instant};

use smithay::utils::{Logical, Point};

use crate::compositor::{App, HitTarget, shell_integration::command_socket_path};

mod contact_state;
mod model;
mod pointer;
mod touch;
pub(in crate::compositor) use model::FavoriteDrawing;
use model::{Arm, Source};

impl App {
    fn favorite_geometry(&self, index: usize) -> Option<[f64; 5]> {
        let origin = self.launcher_logical_origin(index)?;
        let size = self.window_geometry_size(index);
        Some([
            f64::from(origin.x),
            f64::from(origin.y),
            self.window_render_scale(index).max(1.0),
            f64::from(size.w),
            f64::from(size.h),
        ])
    }

    fn favorite_focused_index(&self) -> Option<usize> {
        if !self.navigation_available() {
            return None;
        }
        let index = self.focused_shell_panel_index()?;
        let window = &self.windows[index];
        let output = self.output_logical_rect_for_name(window.shell_output_name.as_deref())?;
        (window.mapped && self.is_search_panel(index) && self.shell_visible_on_output(output))
            .then_some(index)
    }

    pub(in crate::compositor) fn arm_favorite_drawing(
        &mut self,
        generation: u64,
        bounds: [f64; 4],
    ) -> Result<(), String> {
        let index = self
            .favorite_focused_index()
            .ok_or_else(|| "drawing requires focused, active shell search".to_string())?;
        let geometry = self
            .favorite_geometry(index)
            .ok_or("drawing output unavailable")?;
        let [x, y, width, height] = bounds;
        if generation == 0
            || !bounds.iter().all(|n| n.is_finite())
            || x < 0.0
            || y < 0.0
            || width < 16.0
            || height < 16.0
            || x + width > geometry[3]
            || y + height > geometry[4]
        {
            return Err("drawing area must fit inside the search window".into());
        }
        self.favorite_drawing.arm(Arm {
            generation,
            window: self.windows[index].id,
            output: self.windows[index].shell_output_name.clone(),
            bounds,
            geometry,
        })?;
        self.flush_favorite_events();
        Ok(())
    }

    pub(in crate::compositor) fn disarm_favorite_drawing(
        &mut self,
        generation: u64,
    ) -> Result<(), String> {
        self.favorite_drawing.disarm(generation)?;
        self.flush_favorite_events();
        Ok(())
    }

    pub(in crate::compositor) fn accept_favorite_drawing(
        &mut self,
        generation: u64,
        stroke: u64,
    ) -> Result<(), String> {
        self.maintain_favorite_drawing();
        self.favorite_drawing
            .accept(generation, stroke, Instant::now())
    }

    pub(in crate::compositor) fn launch_favorite_drawing(
        &mut self,
        generation: u64,
        stroke: u64,
        app_id: &str,
    ) -> Result<(), String> {
        self.accept_favorite_drawing(generation, stroke)?;
        let output = self
            .favorite_drawing
            .arm
            .as_ref()
            .and_then(|arm| arm.output.clone());
        // This handler runs atomically in the compositor event loop: no focus,
        // lock or seat switch can occur between authorization and safe launch.
        self.launch_app(app_id, output.as_deref())
    }

    pub(in crate::compositor) fn maintain_favorite_drawing(&mut self) {
        if let Some(arm) = self.favorite_drawing.arm.as_ref() {
            let index = self
                .favorite_focused_index()
                .filter(|index| self.windows[*index].id == arm.window);
            match index {
                None => self.favorite_drawing.clear(),
                Some(index) if self.favorite_geometry(index) != Some(arm.geometry) => {
                    self.favorite_drawing.reconfigure();
                }
                _ => (),
            }
        }
        self.flush_favorite_events();
    }

    /// Called from inside Smithay's keyboard focus lock, so never query it here.
    pub(in crate::compositor) fn favorite_focus_changed(&mut self, window: Option<u64>) {
        if self
            .favorite_drawing
            .arm
            .as_ref()
            .is_some_and(|arm| Some(arm.window) != window)
        {
            self.clear_favorite_drawing();
        }
    }

    pub(in crate::compositor) fn favorite_window_unmapped(&mut self, window: u64) {
        if self
            .favorite_drawing
            .arm
            .as_ref()
            .is_some_and(|arm| arm.window == window)
        {
            self.clear_favorite_drawing();
        }
    }

    pub(in crate::compositor) fn clear_favorite_drawing(&mut self) {
        self.favorite_drawing.clear();
        self.flush_favorite_events();
    }

    fn favorite_local(&self, location: Point<f64, Logical>) -> Option<[f64; 2]> {
        let arm = self.favorite_drawing.arm.as_ref()?;
        let HitTarget::Client { window_index, .. } = self.hit_test(location)? else {
            return None;
        };
        if self.windows[window_index].id != arm.window {
            return None;
        }
        arm.local([location.x, location.y])
    }

    fn flush_favorite_events(&mut self) {
        for event in self.favorite_drawing.events() {
            let sent = (|| -> std::io::Result<()> {
                let bytes = serde_json::to_vec(&event)?;
                let socket = UnixDatagram::unbound()?;
                socket.set_nonblocking(true)?;
                socket.send_to(
                    &bytes,
                    hearthspace_common::shell_event_socket_path(&command_socket_path()),
                )?;
                Ok(())
            })();
            if let Err(error) = sent {
                // A missing packet must never allow a truncated stroke to launch.
                // The shell also checks strictly consecutive event sequences.
                tracing::debug!(%error, "favorite drawing event delivery failed");
                self.favorite_drawing.cancel();
                self.favorite_drawing.events();
                break;
            }
        }
    }
}
