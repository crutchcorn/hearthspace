//! Read-through syntax only: output resolution and eligibility remain live App decisions.
use std::cell::RefCell;

use hearthspace_common::{desktop_item_id_from_app_id, frame_layout::*};

use super::{ManagedWindow, window_app_id};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::compositor) enum FrameKind {
    Widget,
    Group,
    Grip,
}

#[derive(Debug, PartialEq, Eq)]
enum Syntax {
    Frame {
        kind: FrameKind,
        output: Option<String>,
        target: String,
    },
    DesktopItem(String),
    Other,
}

impl Syntax {
    fn parse(raw: Option<&str>) -> Self {
        let Some(raw) = raw else {
            return Self::Other;
        };
        let frame = frame_widget_from_app_id(raw)
            .map(|identity| (FrameKind::Widget, identity))
            .or_else(|| frame_group_from_app_id(raw).map(|identity| (FrameKind::Group, identity)))
            .or_else(|| {
                frame_group_grip_from_app_id(raw).map(|identity| (FrameKind::Grip, identity))
            });
        if let Some((kind, (output, target))) = frame {
            Self::Frame {
                kind,
                output,
                target,
            }
        } else {
            desktop_item_id_from_app_id(raw).map_or(Self::Other, Self::DesktopItem)
        }
    }
}

struct Entry {
    raw: Option<String>,
    syntax: Syntax,
}

#[derive(Default)]
pub(in crate::compositor) struct Cache(RefCell<Option<Entry>>);

fn current_syntax(
    entry: &mut Option<Entry>,
    raw: Option<String>,
    parse: impl FnOnce(Option<&str>) -> Syntax,
) -> &Syntax {
    if entry.as_ref().is_none_or(|entry| entry.raw != raw) {
        let syntax = parse(raw.as_deref());
        *entry = Some(Entry { raw, syntax });
    }
    &entry.as_ref().unwrap().syntax
}

impl ManagedWindow {
    fn read_identity<T>(&self, read: impl FnOnce(&Syntax) -> T) -> T {
        // Read the complete current key on every access. XDG unmap resets role
        // properties without app_id_changed; X11 can change class or instance.
        let raw = window_app_id(&self.surface);
        let mut entry = self.identity.0.borrow_mut();
        read(current_syntax(&mut entry, raw, Syntax::parse))
    }

    pub(in crate::compositor) fn frame_identity(
        &self,
        requested: FrameKind,
    ) -> Option<(Option<String>, String)> {
        self.read_identity(|syntax| match syntax {
            Syntax::Frame {
                kind,
                output,
                target,
            } if *kind == requested => Some((output.clone(), target.clone())),
            _ => None,
        })
    }

    pub(in crate::compositor) fn is_frame_surface(&self) -> bool {
        self.read_identity(|syntax| matches!(syntax, Syntax::Frame { .. }))
    }

    pub(in crate::compositor) fn is_frame_grip(&self) -> bool {
        self.read_identity(|syntax| {
            matches!(
                syntax,
                Syntax::Frame {
                    kind: FrameKind::Grip,
                    ..
                }
            )
        })
    }

    pub(in crate::compositor) fn desktop_item_id(&self) -> Option<String> {
        self.read_identity(|syntax| match syntax {
            Syntax::DesktopItem(id) => Some(id.clone()),
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests;
