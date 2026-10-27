//! Fixed canvas bindings shared by compositor input, tool hints, and Settings.
use crate::command::CanvasTool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CanvasAction {
    Tool(CanvasTool),
    Map,
}

pub struct CanvasShortcut {
    pub action: CanvasAction,
    /// Lowercase Latin key used while Meta is held, without other modifiers.
    pub key: char,
    pub label: &'static str,
    pub description: &'static str,
}

impl CanvasShortcut {
    pub fn chord(&self) -> String {
        format!("Meta+{}", self.key.to_ascii_uppercase())
    }
}

impl CanvasAction {
    pub fn shortcut(self) -> &'static CanvasShortcut {
        CANVAS_SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.action == self)
            .expect("every canvas action has a fixed shortcut")
    }
}

pub const CANVAS_SHORTCUTS: &[CanvasShortcut] = &[
    CanvasShortcut {
        action: CanvasAction::Tool(CanvasTool::Select),
        key: 'v',
        label: "Select desktop items",
        description: "Select and move files, folders, and app shortcuts on the canvas.",
    },
    CanvasShortcut {
        action: CanvasAction::Tool(CanvasTool::Pan),
        key: 'h',
        label: "Pan canvas",
        description: "Drag the canvas to move your view without moving windows.",
    },
    CanvasShortcut {
        action: CanvasAction::Tool(CanvasTool::Search),
        key: 'k',
        label: "Search canvas",
        description: "Find windows, apps, files, and desktop controls.",
    },
    CanvasShortcut {
        action: CanvasAction::Tool(CanvasTool::Group),
        key: 'g',
        label: "Select window group",
        description: "Draw a selection around windows to work with them together.",
    },
    CanvasShortcut {
        action: CanvasAction::Tool(CanvasTool::Layout),
        key: 'a',
        label: "Change layout",
        description: "Place windows and desktop controls on the canvas or frame.",
    },
    CanvasShortcut {
        action: CanvasAction::Map,
        key: 'o',
        label: "Canvas map",
        description: "Open or close Overview to navigate and zoom the canvas.",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_canvas_chords_are_unique_and_preserve_window_shortcuts() {
        let mut keys = std::collections::HashSet::new();
        for binding in CANVAS_SHORTCUTS {
            assert!(binding.key.is_ascii_lowercase());
            assert!(keys.insert(binding.key));
            assert!(!['f', 'm', 'l'].contains(&binding.key));
            assert_eq!(binding.action.shortcut().key, binding.key);
        }
        assert_eq!(keys.len(), 6);
    }
}
