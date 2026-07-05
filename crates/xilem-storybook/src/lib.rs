#[derive(Debug, Clone)]
pub struct StorybookState {
    stories: Vec<Story>,
    selected_story: usize,
}

impl StorybookState {
    pub fn new(stories: Vec<Story>) -> Self {
        assert!(!stories.is_empty(), "storybook requires at least one story");
        Self {
            stories,
            selected_story: 0,
        }
    }

    pub fn stories(&self) -> &[Story] {
        &self.stories
    }

    pub fn selected_story_index(&self) -> usize {
        self.selected_story
    }

    pub fn select_story(&mut self, index: usize) {
        if index < self.stories.len() {
            self.selected_story = index;
        }
    }

    pub fn selected_story(&self) -> &Story {
        &self.stories[self.selected_story]
    }

    pub fn selected_story_mut(&mut self) -> &mut Story {
        &mut self.stories[self.selected_story]
    }

    pub fn selected_text(&self, id: &str) -> Option<&str> {
        self.selected_story().text(id)
    }

    pub fn selected_bool(&self, id: &str) -> Option<bool> {
        self.selected_story().bool(id)
    }

    pub fn set_selected_text(&mut self, id: &str, value: String) {
        self.selected_story_mut().set_text(id, value);
    }

    pub fn set_selected_bool(&mut self, id: &str, value: bool) {
        self.selected_story_mut().set_bool(id, value);
    }
}

#[derive(Debug, Clone)]
pub struct Story {
    id: String,
    name: String,
    description: String,
    controls: Vec<Control>,
}

impl Story {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        controls: Vec<Control>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            controls,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    pub fn text(&self, id: &str) -> Option<&str> {
        self.control(id).and_then(Control::as_text)
    }

    pub fn bool(&self, id: &str) -> Option<bool> {
        self.control(id).and_then(Control::as_bool)
    }

    pub fn control(&self, id: &str) -> Option<&Control> {
        self.controls.iter().find(|control| control.id() == id)
    }

    fn control_mut(&mut self, id: &str) -> Option<&mut Control> {
        self.controls.iter_mut().find(|control| control.id() == id)
    }

    fn set_text(&mut self, id: &str, value: String) {
        if let Some(Control::Text(control)) = self.control_mut(id) {
            control.value = value;
        }
    }

    fn set_bool(&mut self, id: &str, value: bool) {
        if let Some(Control::Bool(control)) = self.control_mut(id) {
            control.value = value;
        }
    }
}

#[derive(Debug, Clone)]
pub enum Control {
    Text(TextControl),
    Bool(BoolControl),
}

impl Control {
    pub fn text(id: impl Into<String>, label: impl Into<String>, value: impl Into<String>) -> Self {
        Self::Text(TextControl {
            id: id.into(),
            label: label.into(),
            value: value.into(),
        })
    }

    pub fn bool(id: impl Into<String>, label: impl Into<String>, value: bool) -> Self {
        Self::Bool(BoolControl {
            id: id.into(),
            label: label.into(),
            value,
        })
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Text(control) => &control.id,
            Self::Bool(control) => &control.id,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Text(control) => &control.label,
            Self::Bool(control) => &control.label,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(control) => Some(&control.value),
            Self::Bool(_) => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Text(_) => None,
            Self::Bool(control) => Some(control.value),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TextControl {
    id: String,
    label: String,
    value: String,
}

impl TextControl {
    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone)]
pub struct BoolControl {
    id: String,
    label: String,
    value: bool,
}

impl BoolControl {
    pub fn value(&self) -> bool {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_controls_can_be_updated() {
        let mut state = StorybookState::new(vec![Story::new(
            "button",
            "Button",
            "A button story",
            vec![
                Control::text("label", "Label", "Save"),
                Control::bool("selected", "Selected", false),
            ],
        )]);

        state.set_selected_text("label", "Open".to_string());
        state.set_selected_bool("selected", true);

        assert_eq!(state.selected_text("label"), Some("Open"));
        assert_eq!(state.selected_bool("selected"), Some(true));
    }
}
