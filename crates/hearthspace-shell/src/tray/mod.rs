//! Session-local StatusNotifierItem and DBusMenu integration.
//!
//! Use an explicit Hearthspace bus address, never the ambient desktop bus.
//! The upstream system-tray 0.8.9 client currently accepts only the ambient
//! bus, detaches its tasks, and collapses different object paths to one bus
//! destination. A small zbus host preserves connection ownership and complete
//! item identities without vendoring or patching the upstream crate.

mod actions;
mod backend;
mod menu;
mod model;
mod properties;
mod session;
mod watcher;

pub use backend::{TrayHandle, TrayWorker, channel};
pub use model::*;

#[cfg(test)]
pub(crate) mod tests;
