//! The reference's browser sans-serif resolves to Liberation Sans on Linux.
use xilem::{masonry::core::ArcStr, view::Label};

pub(crate) const FONT: &str = "Liberation Sans";

pub(crate) fn label(text: impl Into<ArcStr>) -> Label {
    xilem::view::label(text).font(FONT)
}
