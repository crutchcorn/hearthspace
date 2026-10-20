use super::{Choice, PickerAction, layer::PickerLayer, option::Artwork};
use crate::favorites_search::{Editor, Navigation};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::masonry::{
    accesskit::{Node, Role},
    core::{
        AccessCtx, ActionCtx, ChildrenIds, ComposeCtx, ErasedAction, EventCtx, LayerType,
        LayoutCtx, MeasureCtx, PaintCtx, PointerEvent, PropertiesMut, PropertiesRef, RegisterCtx,
        TextEvent, Update, UpdateCtx, Widget, WidgetId, WidgetMut, WidgetPod,
        keyboard::{Key, NamedKey},
    },
    imaging::Painter,
    kurbo::{Axis, Point, Rect, Size, Stroke},
    layout::{AsUnit, LenReq, Length},
    properties::{Background, BorderColor, BorderWidth, Padding},
    widgets::{Button, ButtonPress, TextAction},
};

struct Popup {
    layer: WidgetId,
    menu: WidgetId,
    options: Vec<WidgetId>,
    active: usize,
    anchor: Rect,
}

pub(super) struct PickerField {
    selected: Choice,
    choices: Vec<Choice>,
    editor: WidgetPod<Editor>,
    artwork: WidgetPod<Artwork>,
    trigger: WidgetPod<Button>,
    popup: Option<Popup>,
}

impl PickerField {
    pub fn new(selected: Choice, choices: Vec<Choice>) -> Self {
        Self {
            editor: Editor::new(&selected.name).prepare().to_pod(),
            artwork: WidgetPod::new(Artwork::new(selected.image.clone())),
            trigger: Button::new(
                icon(Icon::ChevronDown, 16.0)
                    .label("Choose app")
                    .into_widget()
                    .prepare()
                    .with_class("settings-muted"),
            )
            .prepare()
            .with_props((
                Padding::ZERO,
                Background::Color(xilem::Color::TRANSPARENT),
                BorderWidth { width: 0.0.px() },
                BorderColor {
                    color: xilem::Color::TRANSPARENT,
                },
            ))
            .to_pod(),
            selected,
            choices,
            popup: None,
        }
    }

    pub fn dismiss(this: &mut WidgetMut<'_, Self>) {
        if let Some(popup) = this.widget.popup.take() {
            this.ctx.remove_layer(popup.layer);
            this.ctx.request_paint_only();
            let mut editor = this.ctx.get_mut(&mut this.widget.editor);
            editor.widget.expanded = false;
            editor.widget.menu = None;
            editor.widget.active = None;
            editor.ctx.request_accessibility_update();
        }
    }

    pub fn finish(this: &mut WidgetMut<'_, Self>, id: Option<String>) {
        Self::dismiss(this);
        if let Some(id) = id {
            this.ctx
                .submit_action::<PickerAction>(PickerAction::Choose(id));
        } else {
            let name = this.widget.selected.name.clone();
            {
                let mut editor = this.ctx.get_mut(&mut this.widget.editor);
                editor.widget.reset_text(&name);
                editor.ctx.request_layout();
            }
            this.ctx
                .submit_action::<PickerAction>(PickerAction::Dismiss);
        }
    }

    pub fn update_anchor(this: &mut WidgetMut<'_, Self>) {
        let anchor = this
            .ctx
            .window_transform()
            .transform_rect_bbox(this.ctx.border_box());
        if let Some(popup) = &mut this.widget.popup
            && popup.anchor != anchor
        {
            popup.anchor = anchor;
            this.ctx.mutate_later(popup.layer, move |mut layer| {
                PickerLayer::set_anchor(&mut layer.downcast(), anchor);
            });
        }
    }

    fn sync_accessibility(this: &mut WidgetMut<'_, Self>) {
        let mut editor = this.ctx.get_mut(&mut this.widget.editor);
        editor.widget.expanded = this.widget.popup.is_some();
        editor.widget.menu = this.widget.popup.as_ref().map(|popup| popup.menu);
        editor.widget.active = this
            .widget
            .popup
            .as_ref()
            .and_then(|popup| popup.options.get(popup.active))
            .copied();
        editor.ctx.request_accessibility_update();
    }

    fn open(this: &mut WidgetMut<'_, Self>, active: usize) {
        let anchor = this
            .ctx
            .window_transform()
            .transform_rect_bbox(this.ctx.border_box());
        let layer = PickerLayer::new(
            this.ctx.widget_id(),
            this.widget.editor.id(),
            anchor,
            &this.widget.choices,
            &this.widget.selected.id,
            active,
        );
        let options = layer.option_ids();
        let menu = layer.menu_id();
        let layer = layer.prepare();
        this.widget.popup = Some(Popup {
            layer: layer.id(),
            menu,
            options,
            active,
            anchor,
        });
        this.ctx
            .create_attached_layer(LayerType::Other, layer, Point::ORIGIN);
        this.ctx.request_paint_only();
        Self::sync_accessibility(this);
    }

    pub fn refresh(
        this: &mut WidgetMut<'_, Self>,
        selected: Choice,
        choices: Vec<Choice>,
        open: bool,
        query_changed: bool,
    ) {
        let changed = this.widget.choices != choices;
        let active_id = this
            .widget
            .popup
            .as_ref()
            .and_then(|popup| this.widget.choices.get(popup.active))
            .map(|choice| choice.id.clone());
        if this.widget.selected != selected {
            {
                let mut artwork = this.ctx.get_mut(&mut this.widget.artwork);
                artwork.widget.image = selected.image.clone();
                artwork.ctx.request_paint_only();
            }
            if this.widget.selected.id != selected.id {
                let mut editor = this.ctx.get_mut(&mut this.widget.editor);
                editor.widget.reset_text(&selected.name);
                editor.ctx.request_layout();
            }
            this.widget.selected = selected;
        }
        this.widget.choices = choices;
        if !open {
            Self::dismiss(this);
            let name = this.widget.selected.name.clone();
            let mut editor = this.ctx.get_mut(&mut this.widget.editor);
            if editor.widget.text.text() != name.as_str() {
                editor.widget.reset_text(&name);
                editor.ctx.request_layout();
            }
        } else if this.widget.popup.is_none() || changed || query_changed {
            let active = if query_changed {
                0
            } else {
                let id = active_id.as_deref().unwrap_or(&this.widget.selected.id);
                this.widget
                    .choices
                    .iter()
                    .position(|choice| choice.id == id)
                    .unwrap_or(0)
            };
            Self::dismiss(this);
            Self::open(this, active);
        }
    }

    fn navigate(this: &mut WidgetMut<'_, Self>, step: i32) {
        let count = this.widget.choices.len();
        let Some(popup) = &mut this.widget.popup else {
            return;
        };
        if count == 0 {
            return;
        }
        popup.active = (popup.active as i32 + step).clamp(0, count as i32 - 1) as usize;
        let active = popup.active;
        this.ctx.mutate_later(popup.layer, move |mut layer| {
            PickerLayer::highlight(&mut layer.downcast(), active);
        });
        Self::sync_accessibility(this);
    }
}

mod widget;
