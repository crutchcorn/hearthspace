//! Route bound clients through wl_touch while retaining compositor gesture ownership.
use std::time::Instant;

use smithay::{
    backend::input::{ButtonState, InputTime, TouchSlot},
    input::touch::{DownEvent, MotionEvent, UpEvent},
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER, Serial},
};
use wayland_server::{Resource, protocol::wl_surface::WlSurface};

use super::touch_tracker::TouchAction;
use crate::compositor::{
    App, HitTarget, ManagedSurface, ManagedWindowKind, idle::ActivityReason, windows::window_app_id,
};

mod frame_scroll;
mod groups;
mod lifecycle;
mod model;
mod operation;
use model::{BarAction, BarSwipe};
use operation::WindowOperation;

type Position = Point<f64, Logical>;
type RunningAppIdentity = Vec<(u64, Option<String>)>;

enum Target {
    Group,
    GroupCancelled,
    Client(ClientTouch),
    WindowOperation(WindowOperation),
    FrameScroll {
        target: crate::compositor::layout_editor::Target,
        last: Position,
    },
    Bar(BarSwipe),
    Cancelled,
}

struct ClientTouch {
    surface: WlSurface,
    serial: Serial,
    location: Position,
}

pub(in crate::compositor) struct UiTouch {
    device: String,
    slot: TouchSlot,
    window: u64,
    geometry: [f64; 5],
    output: Rectangle<i32, Logical>,
    origin: Position,
    local: Position,
    target: Target,
    apps_identity: Option<RunningAppIdentity>,
}

impl UiTouch {
    fn local_position(&self, position: Position) -> Position {
        let delta = position - self.origin;
        self.local + Position::from((delta.x / self.geometry[2], delta.y / self.geometry[2]))
    }

    fn owns(&self, device: &str, slot: TouchSlot) -> bool {
        self.device == device && self.slot == slot
    }

    fn cancel(&mut self) -> bool {
        if let Target::WindowOperation(operation) = &mut self.target {
            operation.active = false;
            return false;
        }
        let client = matches!(self.target, Target::Client(_));
        self.target = if matches!(self.target, Target::Group | Target::GroupCancelled) {
            Target::GroupCancelled
        } else {
            Target::Cancelled
        };
        client
    }

    fn valid(
        &self,
        geometry: Option<[f64; 5]>,
        outputs: &[Rectangle<i32, Logical>],
        available: bool,
        apps_identity: Option<&[(u64, Option<String>)]>,
    ) -> bool {
        available
            && geometry == Some(self.geometry)
            && outputs.contains(&self.output)
            && self.apps_identity.as_deref() == apps_identity
    }

    fn suppresses_pointer(&self, device: &str, action: &TouchAction) -> bool {
        self.device == device
            && matches!(
                action,
                TouchAction::Hover(_)
                    | TouchAction::Press(_)
                    | TouchAction::Motion(_)
                    | TouchAction::Release
                    | TouchAction::Tap(_)
            )
    }
}

impl App {
    pub(super) fn begin_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Position,
        time: InputTime,
        first: bool,
    ) {
        if !first {
            // A client receives wl_touch.cancel before the existing tracker
            // recognizes any multi-finger canvas gesture.
            if self
                .ui_touch
                .as_ref()
                .is_some_and(|touch| touch.device == device && touch.slot != slot)
            {
                self.cancel_ui_touch();
            }
            return;
        }
        if self.has_window_touch_operation() {
            return;
        }
        if !self.modal_confirmation_active()
            && self.begin_group_ui_touch(device, slot, location, time)
        {
            return;
        }
        if !self.navigation_available()
            && !self.canvas_search_active()
            && !self.layout_editing()
            && !self.modal_confirmation_active()
        {
            return;
        }
        let Some(HitTarget::Client {
            window_index,
            surface,
            surface_location,
        }) = self.hit_test(location)
        else {
            return;
        };
        if self.layout_editing()
            && !window_app_id(&self.windows[window_index].surface).is_some_and(|id| {
                hearthspace_common::frame_layout::layout_editor_from_app_id(&id).is_some()
            })
            && !self.modal_confirmation_surface(window_index)
        {
            return;
        }
        let bar = self.windows[window_index].kind == ManagedWindowKind::ShellBar
            && !self.is_layout_surface(window_index);
        let supported = self.is_layout_surface(window_index)
            || self
                .layout_scroll_group_at(hearthspace_common::frame_layout::Point {
                    x: location.x,
                    y: location.y,
                })
                .is_some()
            || bar
            || self.is_shell_panel(window_index);
        let bound = surface
            .client()
            .is_some_and(|client| self.touch.client_touch(&client).next().is_some());
        // XWayland's binding cannot identify which X11 clients support native
        // touch. Keep their established pointer fallback, including core clients.
        if !matches!(self.windows[window_index].surface, ManagedSurface::Xdg(_))
            || (!supported
                && !(bound && self.windows[window_index].kind == ManagedWindowKind::Normal))
        {
            return;
        }
        let Some(output) = self
            .output_logical_rects()
            .into_iter()
            .find(|rect| rect.contains(location.to_i32_round()))
        else {
            return;
        };
        let window = self.windows[window_index].id;
        let geometry = self.ui_touch_geometry(window_index);
        let local = location - surface_location;
        let serial = SERIAL_COUNTER.next_serial();
        if !bar {
            let index = self.raise_window(window_index);
            if let Some(root) = self.windows[index].surface.wl_surface() {
                self.set_keyboard_focus_to_window(index, root);
            }
            self.record_client_activity_for_window_index(index, ActivityReason::ClientInput);
            self.touch.clone().down(
                self,
                Some((surface.clone(), (0.0, 0.0).into())),
                &DownEvent {
                    slot,
                    location: local,
                    serial,
                    time,
                },
            );
        }
        self.ui_touch = Some(UiTouch {
            device: device.into(),
            slot,
            window,
            geometry,
            output,
            origin: location,
            local,
            apps_identity: (bar
                && self.shell_corner(window_index) == Some(hearthspace_common::ShellCorner::Apps))
            .then(|| self.running_app_touch_identity()),
            target: if bar {
                Target::Bar(BarSwipe::new(
                    location,
                    Instant::now(),
                    self.shell_corner(window_index),
                ))
            } else {
                Target::Client(ClientTouch {
                    surface: surface.clone(),
                    serial,
                    location,
                })
            },
        });
        if !bar {
            self.record_gtk_touch_down(device, slot, &surface, serial, window);
        }
    }

    pub(super) fn motion_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        location: Position,
        time: InputTime,
    ) {
        self.maintain_ui_touch();
        if self.motion_frame_scroll_ui_touch(device, slot, location) {
            return;
        }
        let Some(touch) = self
            .ui_touch
            .as_mut()
            .filter(|touch| touch.owns(device, slot))
        else {
            return;
        };
        if matches!(touch.target, Target::Group) {
            self.motion_group_ui_touch(location, time);
            return;
        }
        if !location.x.is_finite()
            || !location.y.is_finite()
            || !touch.output.contains(location.to_i32_round())
        {
            self.cancel_ui_touch();
            return;
        }
        let local = touch.local_position(location);
        match &mut touch.target {
            Target::Bar(swipe) => swipe.motion(location),
            Target::Client(client) => {
                client.location = location;
                let surface = client.surface.clone();
                self.touch.clone().motion(
                    self,
                    Some((surface, (0.0, 0.0).into())),
                    &MotionEvent {
                        slot,
                        location: local,
                        time,
                    },
                );
            }
            Target::WindowOperation(_) => self.motion_window_touch_operation(location),
            Target::Group
            | Target::GroupCancelled
            | Target::FrameScroll { .. }
            | Target::Cancelled => (),
        }
    }

    pub(super) fn end_ui_touch(
        &mut self,
        device: &str,
        slot: TouchSlot,
        cancelled: bool,
        time: InputTime,
    ) {
        self.maintain_ui_touch();
        let Some(touch) = self
            .ui_touch
            .as_ref()
            .filter(|touch| touch.device == device && (touch.slot == slot || i32::from(slot) < 0))
        else {
            return;
        };
        if cancelled {
            self.cancel_ui_touch();
            return;
        }
        let origin = touch.origin;
        match &touch.target {
            Target::Group => self.finish_group_ui_touch(time),
            Target::Client(client) => {
                let serial = client.serial;
                let surface = client.surface.clone();
                let owned = self.ui_touch_has_client_grab(&surface, serial);
                self.touch.clone().up(
                    self,
                    &UpEvent {
                        slot,
                        serial: SERIAL_COUNTER.next_serial(),
                        time,
                    },
                );
                self.record_gtk_touch_up(device, slot, serial, owned);
            }
            Target::WindowOperation(_) => {
                self.finish_window_touch_operation();
                return;
            }
            Target::Bar(swipe) => match swipe.finish(Instant::now()) {
                BarAction::OpenSearch => {
                    self.apply_pointer_motion(origin, time);
                    if let Err(error) = self.focus_launcher() {
                        tracing::debug!(%error, "shell swipe could not open apps list");
                    }
                }
                BarAction::Tap => {
                    self.apply_pointer_motion(origin, time);
                    self.touch_pointer_button(ButtonState::Pressed);
                    self.touch_pointer_button(ButtonState::Released);
                }
                BarAction::Cancel => (),
            },
            Target::GroupCancelled | Target::FrameScroll { .. } | Target::Cancelled => (),
        }
        if let Some(touch) = self.ui_touch.as_mut() {
            touch.target = Target::Cancelled;
        }
    }
}

#[cfg(test)]
mod tests;
