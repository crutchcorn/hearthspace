//! A single lock instance and authentication gate, independent of output lifetime.

mod outputs;
mod state;

use std::{cell::RefCell, rc::Rc, time::Duration};

use gtk::{gio, glib, prelude::*};
use gtk4_session_lock::Instance;
use zeroize::Zeroizing;

use crate::{
    auth::{Attempt, Completion, Failure, Identity, Poll},
    config::Config,
};
use outputs::Output;
use state::{Effect, State};

pub(crate) fn run(config: Config) -> glib::ExitCode {
    let identity = match Identity::current() {
        Ok(identity) => identity,
        Err(_) => {
            eprintln!("hearthspace-lock: cannot identify the session's Unix account");
            return glib::ExitCode::FAILURE;
        }
    };
    if gtk::init().is_err() || !gtk4_session_lock::is_supported() {
        eprintln!("hearthspace-lock: a Wayland compositor with session locking is required");
        return glib::ExitCode::FAILURE;
    }
    // No remote activation, command forwarding, quit action or unlock action.
    let application = gtk::Application::builder()
        .application_id("org.hearthspace.Lock")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    let session = Rc::new(Session {
        application: application.clone(),
        instance: Instance::new(),
        config,
        identity,
        state: RefCell::new(State::default()),
        outputs: RefCell::new(Vec::new()),
        attempt: RefCell::new(None),
        hold: RefCell::new(None),
    });
    session.connect();
    let weak = Rc::downgrade(&session);
    application.connect_activate(move |application| {
        let Some(session) = weak.upgrade() else {
            return;
        };
        if session.hold.borrow().is_some() {
            return;
        }
        // Retain even with no monitors. Dropping windows is never an unlock path.
        *session.hold.borrow_mut() = Some(application.hold());
        if !session.instance.lock() {
            session.fail("the compositor rejected the session lock");
        }
    });
    let weak = Rc::downgrade(&session);
    let poll = glib::timeout_add_local(Duration::from_millis(40), move || {
        if let Some(session) = weak.upgrade() {
            session.poll_authentication();
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
    // Main owns argument parsing; no GApplication command can request unlock.
    let exit = application.run_with_args(&["hearthspace-lock"]);
    poll.remove();
    session.clear_secrets();
    if exit == glib::ExitCode::SUCCESS && session.state.borrow().succeeded() {
        glib::ExitCode::SUCCESS
    } else {
        // Native 1.3 keeps callback data for a pending/active lock without a
        // dispose handler. Keep that object valid until process exit, even when
        // zero outputs remain; destroying it must never become cleanup unlock.
        std::mem::forget(session.instance.clone());
        glib::ExitCode::FAILURE
    }
}

struct Session {
    application: gtk::Application,
    instance: Instance,
    config: Config,
    identity: Identity,
    state: RefCell<State>,
    outputs: RefCell<Vec<Output>>,
    attempt: RefCell<Option<Attempt>>,
    hold: RefCell<Option<gio::ApplicationHoldGuard>>,
}

impl Session {
    fn connect(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.instance.connect_locked(move |_| {
            if let Some(session) = weak.upgrade() {
                let effect = session.state.borrow_mut().locked();
                session.apply(effect);
            }
        });
        let weak = Rc::downgrade(self);
        self.instance.connect_failed(move |_| {
            if let Some(session) = weak.upgrade() {
                session.fail("the compositor rejected the session lock");
            }
        });
        let weak = Rc::downgrade(self);
        self.instance.connect_unlocked(move |_| {
            if let Some(session) = weak.upgrade() {
                session.state.borrow_mut().ended();
                if !session.state.borrow().succeeded() {
                    eprintln!("hearthspace-lock: the compositor ended the lock unexpectedly");
                }
                session.clear_secrets();
                session.application.quit();
            }
        });
        let weak = Rc::downgrade(self);
        self.instance.connect_monitor(move |_, monitor| {
            if let Some(session) = weak.upgrade() {
                session.add_output(monitor);
            }
        });
    }

    fn submit(&self, password: Zeroizing<String>) {
        self.clear_secrets();
        let Some(generation) = self.state.borrow_mut().begin_authentication() else {
            return;
        };
        self.update_screens("");
        match Attempt::start(generation, self.identity.clone(), password) {
            Ok(attempt) => *self.attempt.borrow_mut() = Some(attempt),
            Err(failure) => self.complete(Completion {
                generation,
                result: Err(failure),
            }),
        }
    }

    fn poll_authentication(&self) {
        // Release every borrow before invoking GTK or a possibly synchronous signal.
        let result = self.attempt.borrow().as_ref().map(Attempt::poll);
        if let Some(Poll::Complete(completion)) = result {
            self.attempt.borrow_mut().take();
            self.complete(completion);
        }
    }

    fn complete(&self, completion: Completion) {
        let effect = self.state.borrow_mut().authentication_finished(completion);
        self.apply(effect);
    }

    fn apply(&self, effect: Effect) {
        match effect {
            Effect::None => self.update_screens(""),
            Effect::Feedback(failure) => {
                self.clear_secrets();
                self.update_screens(failure.feedback());
            }
            Effect::Unlock => {
                self.clear_secrets();
                // State has observed Locked and successful PAM auth/account checks.
                // This is intentionally the only Instance::unlock call in the app.
                if self.instance.is_locked() {
                    self.instance.unlock();
                } else {
                    self.fail("the compositor lock disappeared before unlock");
                }
            }
        }
    }

    fn fail(&self, reason: &'static str) {
        self.state.borrow_mut().failed();
        self.clear_secrets();
        self.update_screens(Failure::Unavailable.feedback());
        eprintln!("hearthspace-lock: {reason}");
        // No cleanup unlock: disconnect/crash leaves the compositor's shield intact.
        self.application.quit();
    }

    fn clear_secrets(&self) {
        for screen in self.screens() {
            screen.clear_secret();
        }
    }

    fn update_screens(&self, feedback: &str) {
        let busy = self.state.borrow().busy();
        for screen in self.screens() {
            screen.set_feedback(feedback);
            screen.set_busy(busy);
        }
    }

    fn screens(&self) -> Vec<Rc<crate::ui::Screen>> {
        // No borrow may span GTK calls: focus or output callbacks can reenter.
        self.outputs
            .borrow()
            .iter()
            .map(|o| Rc::clone(&o.screen))
            .collect()
    }
}
