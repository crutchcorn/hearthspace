use std::rc::Rc;

use gtk::{gdk, glib, prelude::*};

use crate::ui::Screen;

use super::Session;

pub(super) struct Output {
    pub(super) monitor: gdk::Monitor,
    pub(super) screen: Rc<Screen>,
}

impl Session {
    pub(super) fn add_output(self: &Rc<Self>, monitor: &gdk::Monitor) {
        if !self.state.borrow().accepts_outputs() || !monitor.is_valid() {
            return;
        }
        if self.outputs.borrow().iter().any(|o| o.monitor == *monitor) {
            return;
        }
        self.clear_secrets();
        let weak = Rc::downgrade(self);
        let screen = Screen::new(&self.config, monitor, move |password| {
            // The UI supplies a Zeroizing<String>; even a stale callback erases
            // its secret without creating an unprotected intermediate copy.
            if let Some(session) = weak.upgrade() {
                session.submit(password);
            }
        });
        if screen.window.is_realized() {
            self.fail("a lock window was realized before receiving its lock role");
            return;
        }
        screen.window.set_application(Some(&self.application));
        let busy = self.state.borrow().busy();
        screen.set_busy(busy);
        screen.clear_secret();

        let weak = Rc::downgrade(self);
        screen.window.connect_close_request(move |_| {
            if let Some(session) = weak.upgrade() {
                session.clear_secrets();
            }
            glib::Propagation::Stop
        });
        let weak = Rc::downgrade(self);
        screen.window.connect_is_active_notify(move |_| {
            if let Some(session) = weak.upgrade() {
                session.clear_secrets();
            }
        });
        let weak = Rc::downgrade(self);
        let weak_monitor = monitor.downgrade();
        screen.window.connect_destroy(move |_| {
            if let Some(session) = weak.upgrade() {
                let active = session.state.borrow().accepts_outputs();
                if active && weak_monitor.upgrade().is_some_and(|m| m.is_valid()) {
                    session.fail("a lock window was destroyed unexpectedly");
                }
            }
        });
        let weak = Rc::downgrade(self);
        monitor.connect_invalidate(move |monitor| {
            if let Some(session) = weak.upgrade() {
                session.clear_secrets();
                // The native library unmaps the window. Retain no dead widget or
                // raw pointer for an in-flight authentication result to access.
                let removed = {
                    let mut outputs = session.outputs.borrow_mut();
                    outputs
                        .iter()
                        .position(|o| o.monitor == *monitor)
                        .map(|index| outputs.remove(index))
                };
                drop(removed);
            }
        });

        let window = screen.window.clone();
        self.outputs.borrow_mut().push(Output {
            monitor: monitor.clone(),
            screen: Rc::new(screen),
        });
        // May reenter the main context while presenting. No RefCell borrow spans
        // this call. Library owns configure/resize/scale and presents the window.
        self.instance.assign_window_to_monitor(&window, monitor);
    }
}
