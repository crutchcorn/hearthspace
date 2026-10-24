use smithay::{
    backend::input::{ButtonState, InputTime},
    input::pointer::{ButtonEvent, MotionEvent},
    utils::SERIAL_COUNTER,
};

use crate::compositor::App;

impl App {
    pub(in crate::compositor) fn lock_pointer_motion(&mut self, time: InputTime) {
        let target = self.lock_pointer_target(self.pointer_location);
        self.cursor_visible = target.is_some();
        if target.is_none() {
            self.cursor_surface = None;
        }
        let pointer = self.pointer.clone();
        pointer.motion(
            self,
            target,
            &MotionEvent {
                location: self.pointer_location,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
    }

    pub(in crate::compositor) fn deliver_lock_button(
        &mut self,
        button: u32,
        state: ButtonState,
        time: InputTime,
    ) {
        self.lock_pointer_motion(time);
        if state == ButtonState::Pressed {
            self.focus_lock_surface_at(self.pointer_location);
        }
        let pointer = self.pointer.clone();
        self.deliver_pointer_button(&ButtonEvent {
            serial: SERIAL_COUNTER.next_serial(),
            time,
            button,
            state,
        });
        pointer.frame(self);
    }

    pub(in crate::compositor) fn launch_session_locker(&self) -> Result<(), String> {
        if !self.session_lock.supported {
            return Err("session locking requires the native or headless backend".into());
        }
        let executable = std::env::current_exe()
            .map_err(|error| format!("cannot locate the session locker: {error}"))?
            .with_file_name("hearthspace-lock");
        let child = std::process::Command::new(executable)
            .env(
                crate::config::WAYLAND_DISPLAY_ENV,
                crate::config::wayland_display_name(),
            )
            .env_remove("WAYLAND_SOCKET")
            .env("GDK_BACKEND", "wayland")
            .spawn()
            .map_err(|error| format!("failed to launch Hearthspace's lock screen: {error}"))?;
        hearthspace_desktop::app_catalog::reap_child(child);
        Ok(())
    }
}
