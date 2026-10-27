//! The reference's browser sans-serif resolves to Liberation Sans on Linux.
use std::borrow::Cow;
use xilem::{
    masonry::{
        core::ArcStr,
        parley::{FontFamily, FontFamilyName, GenericFamily},
    },
    view::Label,
};

pub(crate) const FONT: &str = "Liberation Sans";

// Preserve existing text metrics, then use the system UI families for missing
// symbols such as ❖. Parley's script fallback alone does not cover that symbol.
pub(crate) const FONT_FAMILY: FontFamily<'static> = FontFamily::List(Cow::Borrowed(&[
    FontFamilyName::Named(Cow::Borrowed(FONT)),
    FontFamilyName::Generic(GenericFamily::SystemUi),
]));

pub(crate) fn label(text: impl Into<ArcStr>) -> Label {
    xilem::view::label(text).font(FONT_FAMILY)
}
