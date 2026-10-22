//! A configure can reach Winit before input queued against the previous buffer.
//! Hold that click until the same control commits its new local coordinates.
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use smithay::{
    backend::{input::InputTime, renderer::utils::with_renderer_surface_state},
    reexports::calloop::timer::{TimeoutAction, Timer},
    utils::{IsAlive, SERIAL_COUNTER, Serial, Size},
    wayland::{compositor::with_states, shell::xdg::XdgToplevelSurfaceData},
};

use super::*;

const LIFETIME: Duration = Duration::from_secs(2);
const MAX_EVENTS: usize = 128;

#[derive(Debug)]
enum Event {
    Motion(MotionEvent),
    Button(ButtonEvent, Point<f64, Logical>),
}

#[derive(Debug)]
pub(super) struct Pending {
    events: Vec<Event>,
    started: Instant,
    cancelled: AtomicBool,
    reconciled: AtomicBool,
}

impl Pending {
    fn new() -> Self {
        Self {
            events: Vec::new(),
            started: Instant::now(),
            cancelled: AtomicBool::new(false),
            reconciled: AtomicBool::new(false),
        }
    }

    fn push(&mut self, event: Event) {
        if self.cancelled.load(Ordering::Relaxed) {
            return;
        }
        if self.events.len() >= MAX_EVENTS {
            self.cancelled.store(true, Ordering::Relaxed);
            self.events.clear();
        } else {
            self.events.push(event);
        }
    }

    pub(super) fn motion(&mut self, event: MotionEvent) {
        if let Some(Event::Motion(previous)) = self.events.last_mut() {
            *previous = event;
        } else {
            self.push(Event::Motion(event));
        }
    }

    pub(super) fn button(&mut self, event: ButtonEvent, point: Point<f64, Logical>) {
        self.push(Event::Button(event, point));
    }
}

impl App {
    fn shell_resize_state(&self, index: usize) -> Option<(Serial, Size<i32, Logical>, bool)> {
        let surface = self.windows[index].surface.xdg_toplevel()?;
        let (serial, size) = with_states(surface.wl_surface(), |states| {
            let role = states
                .data_map
                .get::<XdgToplevelSurfaceData>()?
                .lock()
                .ok()?;
            let configured = role
                .pending_configures()
                .last()
                .or(role.last_acked.as_ref())?;
            Some((configured.serial, configured.state.size?))
        })?;
        let committed = surface.with_cached_state(|state| {
            state
                .last_acked
                .as_ref()
                .is_some_and(|state| state.serial >= serial)
        });
        let surface_size =
            with_renderer_surface_state(surface.wl_surface(), |state| state.surface_size())
                .flatten();
        Some((
            serial,
            size,
            committed && self.window_geometry_size(index) == size && surface_size == Some(size),
        ))
    }

    pub(in crate::compositor::input) fn defer_shell_resize_click(
        &mut self,
        focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &ButtonEvent,
    ) {
        let Some((surface, origin)) = focus else {
            return;
        };
        let Some(index) = self.window_index_for_surface(&surface) else {
            return;
        };
        if self.frame_widget_identity(index).is_none() && self.shell_corner(index).is_none() {
            return;
        }
        let Some((_, size, _)) = self.shell_resize_state(index) else {
            return;
        };
        let root_size = self.windows[index].surface.wl_surface().and_then(|root| {
            with_renderer_surface_state(&root, |state| state.surface_size()).flatten()
        });
        if self.window_geometry_size(index) == size && root_size == Some(size) {
            return;
        }
        let Some(local) = self.shell_grab_local(index, self.pointer_location) else {
            return;
        };
        let serial = event.serial;
        let pointer = self.pointer.clone();
        pointer.set_grab(
            self,
            ShellGrab {
                window_id: self.windows[index].id,
                child_offset: local - (self.pointer_location - origin),
                original_focus: (surface, origin),
                // A dead pending owner must discard the sequence, not let Smithay
                // fall back to its default grab and forward an unmatched release.
                start: GrabStartData {
                    focus: None,
                    button: event.button,
                    location: self.pointer_location,
                },
                pending: Some(Pending::new()),
            },
            serial,
            Focus::Keep,
        );
        let timer = Timer::from_duration(LIFETIME);
        if self
            .loop_handle
            .insert_source(timer, move |_, _, data| {
                if data.state.pointer.has_grab(serial) {
                    data.state.refresh_pending_shell_click();
                }
                TimeoutAction::Drop
            })
            .is_err()
        {
            // The first physical Down has not entered Smithay yet. Keep the
            // cancelled grab installed so it consumes that pair too.
            self.mark_pending_shell_click_cancelled();
        }
    }

    pub(in crate::compositor) fn refresh_pending_shell_click(&mut self) {
        let pointer = self.pointer.clone();
        if !pointer
            .with_grab(|_, grab| {
                grab.downcast_ref::<ShellGrab>()
                    .and_then(|grab| grab.pending.as_ref())
                    .is_some_and(|pending| {
                        pending.reconciled.store(true, Ordering::Relaxed);
                        true
                    })
            })
            .unwrap_or(false)
        {
            return;
        }
        let focus = match self.hit_test(self.pointer_location) {
            Some(crate::compositor::HitTarget::Client {
                surface,
                surface_location,
                ..
            }) => Some((surface, surface_location)),
            _ => None,
        };
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time: InputTime::now(),
            },
        );
        pointer.frame(self);
    }

    pub(in crate::compositor) fn cancel_pending_shell_click(&mut self) {
        self.mark_pending_shell_click_cancelled();
        self.refresh_pending_shell_click();
    }

    fn mark_pending_shell_click_cancelled(&self) {
        self.pointer.with_grab(|_, grab| {
            if let Some(pending) = grab
                .downcast_ref::<ShellGrab>()
                .and_then(|grab| grab.pending.as_ref())
            {
                pending.cancelled.store(true, Ordering::Relaxed);
            }
        });
    }

    pub(in crate::compositor::input) fn abandon_released_shell_resize_click(&mut self) {
        if !self.pressed_pointer_buttons.is_empty() {
            return;
        }
        let owner = self
            .pointer
            .with_grab(|_, grab| {
                grab.downcast_ref::<ShellGrab>()
                    .filter(|grab| grab.pending.is_some())
                    .map(|grab| grab.window_id)
            })
            .flatten();
        if let Some(owner) = owner {
            let still_over_owner = matches!(self.hit_test(self.pointer_location),
                Some(crate::compositor::HitTarget::Client { window_index, .. }) if self.windows[window_index].id == owner);
            if !still_over_owner {
                self.cancel_pending_shell_click();
            }
        }
    }
}

impl ShellGrab {
    pub(super) fn flush_pending(
        &mut self,
        data: &mut App,
        handle: &mut PointerInnerHandle<'_, App>,
        serial: Serial,
        time: InputTime,
    ) {
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        let index = data
            .windows
            .iter()
            .position(|window| window.id == self.window_id && window.mapped);
        if pending.started.elapsed() >= LIFETIME
            || data.session_locked()
            || !self.original_focus.0.alive()
            || index.is_none()
        {
            pending.cancelled.store(true, Ordering::Relaxed);
        }
        if pending.cancelled.load(Ordering::Relaxed) {
            pending.events.clear();
            if handle.current_pressed().is_empty() {
                // Suppressed motion did not update Smithay's last location.
                // Restore hover at the real pointer, without replaying input
                // to the cancelled owner.
                handle.motion(
                    data,
                    None,
                    &MotionEvent {
                        location: data.pointer_location,
                        serial,
                        time,
                    },
                );
                handle.unset_grab(self, data, serial, time, true);
            }
            return;
        }
        if !pending.reconciled.swap(false, Ordering::Relaxed)
            || !index
                .and_then(|index| data.shell_resize_state(index))
                .is_some_and(|(_, _, ready)| ready)
        {
            return;
        }
        let pending = self.pending.take().unwrap();
        self.start.focus = Some(self.original_focus.clone());
        for event in pending.events {
            let (motion, button) = match event {
                Event::Motion(event) => (event, None),
                Event::Button(event, location) => (
                    MotionEvent {
                        location,
                        serial: event.serial,
                        time: event.time,
                    },
                    Some(event),
                ),
            };
            handle.motion(data, self.focus(data, motion.location), &motion);
            if let Some(event) = button {
                handle.button(data, &event);
            }
            handle.frame(data);
        }
        if handle.current_pressed().is_empty() {
            handle.unset_grab(self, data, serial, time, true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_click_overflow_discards_the_entire_sequence() {
        let mut pending = Pending::new();
        let event = ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time: InputTime::now(),
            button: 272,
            state: smithay::backend::input::ButtonState::Pressed,
        };
        for _ in 0..=MAX_EVENTS {
            pending.button(event, (10.0, 10.0).into());
        }
        assert!(pending.cancelled.load(Ordering::Relaxed));
        assert!(pending.events.is_empty());
        pending.button(event, (20.0, 20.0).into());
        assert!(pending.events.is_empty());
    }
}
