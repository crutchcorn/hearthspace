//! ext-session-lock policy. The compositor owns isolation and presentation;
//! the lock client owns authentication. Losing the client never unlocks.
use smithay::{
    backend::input::InputTime,
    desktop::{PopupUngrabStrategy, WindowSurfaceType, utils::under_from_surface_tree},
    input::pointer::MotionEvent,
    output::Output,
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
    wayland::session_lock::{
        LockSurface, SessionLockHandler, SessionLockManagerState, SessionLocker,
    },
};
use wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::ExtSessionLockV1;
use wayland_server::{
    DisplayHandle, Resource,
    protocol::{wl_output::WlOutput, wl_surface::WlSurface},
};

use super::{App, focus::KeyboardFocusTarget};

mod barrier;
mod input;
mod surfaces;

pub(super) struct LockState {
    manager: SessionLockManagerState,
    supported: bool,
    barrier: barrier::PresentationBarrier,
    pending: Option<SessionLocker>,
    owner: Option<ExtSessionLockV1>,
    surfaces: Vec<(String, LockSurface)>,
    ready_surfaces: Vec<WlSurface>,
    previous_focus: Option<u64>,
}

impl LockState {
    pub(super) fn new(display: &DisplayHandle, supported: bool) -> Self {
        Self {
            manager: SessionLockManagerState::new::<App, _>(display, move |_| supported),
            supported,
            barrier: Default::default(),
            pending: None,
            owner: None,
            surfaces: Vec::new(),
            ready_surfaces: Vec::new(),
            previous_focus: None,
        }
    }
}

impl SessionLockHandler for App {
    fn lock_state(&mut self) -> &mut SessionLockManagerState {
        &mut self.session_lock.manager
    }

    fn lock(&mut self, confirmation: SessionLocker) {
        if !self.session_lock.supported || self.session_locked() {
            // No takeover after a crashed locker: only the original lock
            // object can authenticate and release this session.
            return;
        }
        self.cancel_input_gestures();
        self.interrupt_pinch_zoom();
        self.viewport_animation = None;
        self.drag = None;
        self.resize = None;
        if let Some(mut grab) = self.popup_grab.take() {
            grab.ungrab(PopupUngrabStrategy::All);
        }
        self.pointer
            .clone()
            .unset_grab(self, SERIAL_COUNTER.next_serial(), InputTime::now());
        self.keyboard.clone().unset_grab(self);
        self.session_lock.previous_focus = self.focused_normal_window_id;
        self.clear_keyboard_focus();
        self.pointer.clone().motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        self.pointer.clone().frame(self);
        self.cursor_surface = None;
        self.cursor_icon = super::cursor::CursorIcon::Default;
        self.cursor_visible = false;
        let outputs = self.capture_outputs().into_iter().map(|output| output.name);
        self.session_lock.barrier.begin(outputs);
        self.session_lock.owner = Some(confirmation.ext_session_lock().clone());
        self.session_lock.pending = Some(confirmation);
        self.confirm_session_lock_if_presented();
        self.request_redraw();
    }

    fn unlock(&mut self) {
        self.pointer
            .clone()
            .unset_grab(self, SERIAL_COUNTER.next_serial(), InputTime::now());
        self.keyboard.clone().unset_grab(self);
        self.pointer.clone().motion(
            self,
            None,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        self.pointer.clone().frame(self);
        self.session_lock.barrier.unlock();
        self.session_lock.pending = None;
        self.session_lock.owner = None;
        self.session_lock.surfaces.clear();
        self.session_lock.ready_surfaces.clear();
        self.clear_keyboard_focus();
        self.cursor_surface = None;
        self.cursor_visible = true;
        if self
            .session_lock
            .previous_focus
            .take()
            .is_none_or(|id| self.focus_window(id).is_err())
        {
            self.restore_window_focus();
        }
        self.request_redraw();
    }

    fn new_surface(&mut self, surface: LockSurface, output: WlOutput) {
        if self.session_lock.owner.as_ref() != Some(surface.ext_session_lock()) {
            return;
        }
        let Some(output) = Output::from_resource(&output) else {
            return;
        };
        let name = output.name();
        let Some(rect) = self.output_logical_rect_for_name(Some(&name)) else {
            return;
        };
        surface.with_pending_state(|state| {
            state.size = Some((rect.size.w.max(1) as u32, rect.size.h.max(1) as u32).into());
        });
        output.enter(surface.wl_surface());
        self.session_lock.surfaces.push((name, surface));
        self.request_redraw();
    }
}

impl App {
    pub(super) fn session_locked(&self) -> bool {
        self.session_lock.barrier.generation().is_some()
    }

    pub(super) fn lock_confirmed(&self) -> bool {
        self.session_lock.barrier.ready() && self.session_lock.pending.is_none()
    }

    pub(super) fn lock_frame_generation(&self) -> Option<u64> {
        self.session_lock.barrier.generation()
    }

    /// Called only after a backend confirms presentation of the tagged frame.
    pub(super) fn lock_frame_presented(&mut self, output: &str, generation: u64) {
        self.session_lock.barrier.presented(output, generation);
        self.confirm_session_lock_if_presented();
    }

    fn confirm_session_lock_if_presented(&mut self) {
        if self.session_lock.barrier.ready()
            && let Some(confirmation) = self.session_lock.pending.take()
            && confirmation.ext_session_lock().is_alive()
        {
            confirmation.lock();
            tracing::info!("session lock presented on every output");
        }
        // Keep the shield even if the locker died before acknowledgement.
    }

    pub(super) fn reconfigure_lock_outputs(&mut self) {
        if !self.session_locked() {
            return;
        }
        let outputs = self.capture_outputs();
        self.session_lock
            .barrier
            .update_outputs(outputs.iter().map(|output| output.name.clone()));
        // A removed output's role must not take keyboard focus or be reused if
        // a new Wayland global later appears with the same connector name.
        self.session_lock
            .surfaces
            .retain(|(name, _)| outputs.iter().any(|output| &output.name == name));
        for (name, surface) in &self.session_lock.surfaces {
            if let Some(output) = outputs.iter().find(|output| &output.name == name) {
                surface.with_pending_state(|state| {
                    state.size = Some(
                        (
                            output.logical_width.max(1) as u32,
                            output.logical_height.max(1) as u32,
                        )
                            .into(),
                    );
                });
                surface.send_configure();
            }
        }
        // With no connected outputs there are no outstanding presentations.
        // Reconnecting still renders only the shield until a new lock surface
        // is configured; the original owning object remains authoritative.
        self.confirm_session_lock_if_presented();
        self.request_redraw();
    }

    pub(super) fn lock_surface_on_output(
        &self,
        rect: Rectangle<i32, Logical>,
    ) -> Option<WlSurface> {
        if !self.session_locked()
            || !self
                .session_lock
                .owner
                .as_ref()
                .is_some_and(Resource::is_alive)
        {
            return None;
        }
        self.session_lock
            .surfaces
            .iter()
            .find_map(|(name, surface)| {
                (self.output_logical_rect_for_name(Some(name)) == Some(rect)
                    && surfaces::ready(surface))
                .then(|| surface.wl_surface().clone())
            })
    }

    pub(super) fn lock_pointer_target(
        &self,
        position: Point<f64, Logical>,
    ) -> Option<(WlSurface, Point<f64, Logical>)> {
        let output = self
            .output_logical_rects()
            .into_iter()
            .find(|rect| rect.contains(position.to_i32_round()))?;
        let root = self.lock_surface_on_output(output)?;
        under_from_surface_tree(&root, position, output.loc, WindowSurfaceType::ALL)
            .map(|(surface, origin)| (surface, origin.to_f64()))
    }

    pub(super) fn focus_lock_surface_at(&mut self, position: Point<f64, Logical>) {
        if let Some((surface, _)) = self.lock_pointer_target(position) {
            self.keyboard.clone().set_focus(
                self,
                Some(KeyboardFocusTarget::Wayland(surface)),
                SERIAL_COUNTER.next_serial(),
            );
        }
    }
}
