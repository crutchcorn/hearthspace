use std::collections::HashMap;

use smithay::{
    backend::input::{
        AbsolutePositionEvent, ButtonState, Event, InputEvent, InputTime, PointerButtonEvent,
    },
    input::Seat,
    utils::{Logical, Point, Rectangle, Transform},
    wayland::virtual_pointer::{
        VirtualPointerBackend, VirtualPointerDevice, VirtualPointerHandler,
    },
};

use crate::compositor::{App, input::handle_input_event};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::compositor::input) enum PointerSource {
    Local,
    Touch,
    Remote(VirtualPointerDevice),
}

/// One logical button remains held until the last contributing device releases.
#[derive(Default)]
pub(super) struct ButtonSources(HashMap<u32, Vec<PointerSource>>);

impl ButtonSources {
    pub(super) fn transition(
        &mut self,
        source: PointerSource,
        button: u32,
        state: ButtonState,
    ) -> bool {
        match state {
            ButtonState::Pressed => {
                let held = self.0.entry(button).or_default();
                if held.contains(&source) {
                    return false;
                }
                held.push(source);
                held.len() == 1
            }
            ButtonState::Released => {
                let Some(held) = self.0.get_mut(&button) else {
                    return false;
                };
                held.retain(|candidate| candidate != &source);
                let released = held.is_empty();
                if held.is_empty() {
                    self.0.remove(&button);
                }
                released
            }
        }
    }

    fn remove_remote(&mut self, device: Option<&VirtualPointerDevice>) -> Vec<u32> {
        self.remove_sources(|source| match source {
            PointerSource::Local | PointerSource::Touch => false,
            PointerSource::Remote(candidate) => device.is_none_or(|device| device == candidate),
        })
    }

    fn remove_local(&mut self) -> Vec<u32> {
        self.remove_sources(|source| matches!(source, PointerSource::Local))
    }

    fn remove_sources(&mut self, remove: impl Fn(&PointerSource) -> bool) -> Vec<u32> {
        let mut released = Vec::new();
        self.0.retain(|button, sources| {
            sources.retain(|source| !remove(source));
            if sources.is_empty() {
                released.push(*button);
            }
            !sources.is_empty()
        });
        released
    }
}

impl VirtualPointerHandler for App {
    fn process_virtual_pointer_event(&mut self, event: InputEvent<VirtualPointerBackend>) {
        if let InputEvent::DeviceAdded { device } = &event {
            if self.remote_pointer_seat(device) {
                self.remote_input
                    .pointers
                    .insert(device.clone(), device.output().map(|output| output.name()));
            }
            return;
        }
        if let InputEvent::DeviceRemoved { device } = &event {
            self.release_remote_pointer(Some(device));
            self.remote_input.pointers.remove(device);
            return;
        }
        if !self.remote_input_available() {
            self.release_remote_pointer_buttons();
            return;
        }
        match event {
            InputEvent::PointerMotionAbsolute { event } => {
                let device = event.device();
                let Some(output_name) = self.remote_input.pointers.get(&device) else {
                    return;
                };
                let (rect, transform) = if let Some(name) = output_name {
                    let Some(rect) = self.output_logical_rect_for_name(Some(name)) else {
                        return;
                    };
                    let Some(output) = device.output() else {
                        return;
                    };
                    (rect, output.current_transform())
                } else {
                    let Some(rect) = layout_bounds(&self.output_logical_rects()) else {
                        return;
                    };
                    (rect, Transform::Normal)
                };
                let location = absolute_location((event.x(), event.y()).into(), rect, transform);
                self.apply_pointer_motion(location, event.time());
            }
            InputEvent::PointerButton { event } => {
                if self.remote_input.pointers.contains_key(&event.device()) {
                    self.handle_pointer_button_from_source(
                        PointerSource::Remote(event.device()),
                        event.button_code(),
                        event.state(),
                        event.time(),
                    );
                }
            }
            InputEvent::PointerMotion { event }
                if self.remote_input.pointers.contains_key(&event.device()) =>
            {
                handle_input_event(
                    self,
                    InputEvent::<VirtualPointerBackend>::PointerMotion { event },
                );
            }
            InputEvent::PointerAxis { event }
                if self.remote_input.pointers.contains_key(&event.device()) =>
            {
                handle_input_event(
                    self,
                    InputEvent::<VirtualPointerBackend>::PointerAxis { event },
                );
            }
            _ => {}
        }
    }
}

impl App {
    pub(in crate::compositor::input) fn has_pressed_pointer_sources(&self) -> bool {
        // Includes compositor-consumed buttons that never reached Smithay.
        !self.remote_input.buttons.0.is_empty()
    }

    fn remote_pointer_seat(&self, device: &VirtualPointerDevice) -> bool {
        self.remote_input.enabled
            && device
                .wl_seat()
                .is_none_or(|seat| Seat::<App>::from_resource(seat).as_ref() == Some(&self._seat))
    }

    pub(in crate::compositor::input) fn handle_pointer_button_from_source(
        &mut self,
        source: PointerSource,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) {
        if state == ButtonState::Pressed {
            // Includes shell-consumed presses and a second input source joining
            // a held button without producing another Wayland press.
            self.gtk_shell.reset_input();
        }
        if self.remote_input.buttons.transition(source, button, state) {
            self.handle_pointer_button_policy(button, state, time);
        }
    }

    pub(super) fn release_remote_pointer_buttons(&mut self) {
        self.release_remote_pointer(None);
    }

    fn release_remote_pointer(&mut self, device: Option<&VirtualPointerDevice>) {
        // Smithay emits synthetic releases before DeviceRemoved, so the source
        // map can already be empty. Destruction still revokes gesture authority.
        self.gtk_shell.reset_input();
        let released = self.remote_input.buttons.remove_remote(device);
        self.release_removed_pointer_buttons(released);
    }

    pub(in crate::compositor::input) fn release_local_pointer(&mut self) {
        self.gtk_shell.reset_input();
        let released = self.remote_input.buttons.remove_local();
        self.release_removed_pointer_buttons(released);
    }

    fn release_removed_pointer_buttons(&mut self, released: Vec<u32>) {
        if !released.is_empty() {
            // Device cleanup releases held buttons without authorizing gestures.
            self.gtk_shell.reset_input();
            self.cancel_pending_shell_click();
            if released.contains(&0x110) {
                self.cancel_titlebar_pointer();
                self.cancel_layout_drag();
            }
            self.favorite_pointer_removed();
            for button in released {
                self.handle_pointer_button_policy(button, ButtonState::Released, InputTime::now());
            }
        }
    }

    pub(in crate::compositor::input) fn clear_pointer_sources(&mut self) {
        self.remote_input.buttons.0.clear();
    }
}

fn absolute_location(
    normalized: Point<f64, Logical>,
    rect: Rectangle<i32, Logical>,
    transform: Transform,
) -> Point<f64, Logical> {
    let normalized = transform.transform_point_in(normalized, &(1.0, 1.0).into());
    let coordinate = |value: f64, size: i32| {
        (value.clamp(0.0, 1.0) * f64::from(size)).min(f64::from(size) - 0.000_001)
    };
    rect.loc.to_f64()
        + Point::from((
            coordinate(normalized.x, rect.size.w),
            coordinate(normalized.y, rect.size.h),
        ))
}

fn layout_bounds(rectangles: &[Rectangle<i32, Logical>]) -> Option<Rectangle<i32, Logical>> {
    let mut bounds = *rectangles.first()?;
    for rectangle in &rectangles[1..] {
        bounds = bounds.merge(*rectangle);
    }
    Some(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_mouse_does_not_release_an_active_touch_sequence() {
        let mut sources = ButtonSources::default();
        assert!(sources.transition(PointerSource::Touch, 0x110, ButtonState::Pressed));
        assert!(!sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(sources.remove_local().is_empty());
        assert!(!sources.transition(PointerSource::Local, 0x110, ButtonState::Released));
        assert!(sources.transition(PointerSource::Touch, 0x110, ButtonState::Released));
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Released));
    }

    #[test]
    fn touch_cancellation_keeps_a_physical_button_held_until_its_release() {
        let mut sources = ButtonSources::default();
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(!sources.transition(PointerSource::Touch, 0x110, ButtonState::Pressed));
        assert!(!sources.transition(PointerSource::Touch, 0x110, ButtonState::Released));
        assert!(!sources.transition(PointerSource::Touch, 0x110, ButtonState::Released));
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Released));
    }

    #[test]
    fn removing_local_pointer_releases_held_buttons_and_allows_the_next_press() {
        let mut sources = ButtonSources::default();
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(!sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(sources.transition(PointerSource::Local, 0x111, ButtonState::Pressed));
        assert!(
            sources.remove_remote(None).is_empty(),
            "remote cleanup must retain local holds"
        );
        let mut released = sources.remove_local();
        released.sort_unstable();
        assert_eq!(released, [0x110, 0x111]);
        assert!(
            sources.remove_local().is_empty(),
            "removal releases each logical button once"
        );
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Pressed));
        assert!(sources.transition(PointerSource::Local, 0x110, ButtonState::Released));
    }

    #[test]
    fn absolute_input_uses_output_logical_extent_and_origin() {
        let rect = Rectangle::new((1920, 300).into(), (1280, 720).into());
        assert_eq!(
            absolute_location((0.5, 0.25).into(), rect, Transform::Normal),
            (2560.0, 480.0).into()
        );
        let edge = absolute_location((1.0, 1.0).into(), rect, Transform::Normal);
        assert!(edge.x < 3200.0 && edge.y < 1020.0);
    }

    #[test]
    fn rotated_absolute_input_tracks_display_orientation() {
        let rect = Rectangle::new((0, 0).into(), (600, 800).into());
        let point = absolute_location((0.25, 0.75).into(), rect, Transform::_90);
        assert_eq!(point, (150.0, 200.0).into());
    }

    #[test]
    fn whole_layout_includes_negative_output_origins() {
        let rects = [
            Rectangle::new((-1280, 100).into(), (1280, 720).into()),
            Rectangle::new((0, 0).into(), (1920, 1080).into()),
        ];
        assert_eq!(
            layout_bounds(&rects),
            Some(Rectangle::new((-1280, 0).into(), (3200, 1080).into()))
        );
        assert!(layout_bounds(&[]).is_none());
    }
}
