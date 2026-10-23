//! Shared identities, persisted layout, and reference frame geometry.
//! Frame attachment is a presentation choice; it never replaces a window's canvas home.
mod collisions;
mod editor;
mod geometry;
mod groups;
mod identity;
mod model;

pub use collisions::merge_overlapping_frame_items;
pub use editor::editor_height;
pub use geometry::*;
pub use identity::*;
pub use model::*;

#[cfg(test)]
mod tests;
