//! Shared desktop services without a UI runtime or shell dependency.
//!
//! Application discovery, favorites, artwork and file-launch helpers are shared
//! by the compositor, shell and Settings. Consumers retain their UI workers and
//! orchestration; the compositor still chooses launch policy and environment.

pub mod activation;
pub mod app_catalog;
pub mod app_icon;
pub mod artwork;
pub mod environment;
pub mod favorites;
pub mod file_launch;
pub mod gpu;
