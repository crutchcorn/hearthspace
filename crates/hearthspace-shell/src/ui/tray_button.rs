//! Xilem adapter for named application icons and menu commands.
use super::{
    state::ShellState,
    tray_state,
    tray_widget::{Appearance, ButtonAction, ButtonEvent, TrayButton},
};
use crate::tray::{ItemKey, TrayCommand, TrayItem, TrayMenuItem, TrayStatus};
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        core::WidgetMut,
        peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
        widgets::Label,
    },
};

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Target {
    Item(ItemKey),
    Dock {
        app: String,
        item: ItemKey,
        menu_path: String,
    },
    Menu(ItemKey, String, i32),
    Back,
    Close,
    DesktopContext(u64, bool, usize),
}

pub(super) struct ButtonView {
    pub output: String,
    pub target: Target,
    pub appearance: Appearance,
}

pub(super) fn icon(output: &str, item: &TrayItem) -> ButtonView {
    ButtonView {
        output: output.into(),
        target: Target::Item(item.key.clone()),
        appearance: Appearance {
            name: item.title.clone(),
            description: item.tooltip.clone(),
            image: image(item),
            icon: true,
            attention: item.status == TrayStatus::NeedsAttention,
            ..Default::default()
        },
    }
}
fn image(item: &TrayItem) -> Option<ImageBrush> {
    let pixmap = item.icon.pixmap.as_ref()?;
    Some(
        ImageData {
            data: pixmap.rgba.clone().into(),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: pixmap.width,
            height: pixmap.height,
        }
        .into(),
    )
}

pub(super) fn menu(
    output: &str,
    key: &ItemKey,
    menu_path: &str,
    item: &TrayMenuItem,
    selected: bool,
) -> ButtonView {
    ButtonView {
        output: output.into(),
        target: Target::Menu(key.clone(), menu_path.to_owned(), item.id),
        appearance: Appearance {
            name: item.label.clone(),
            menu: !item.separator,
            separator: item.separator,
            enabled: item.enabled,
            selected,
            toggle: item.toggle,
            submenu: item.submenu || !item.children.is_empty(),
            ..Default::default()
        },
    }
}

pub(super) fn text(output: &str, name: String, target: Target, selected: bool) -> ButtonView {
    ButtonView {
        output: output.into(),
        target,
        appearance: Appearance {
            name,
            autofocus: selected,
            ..Default::default()
        },
    }
}

impl ViewMarker for ButtonView {}
impl View<ShellState, (), ViewCtx> for ButtonView {
    type Element = Pod<TrayButton>;
    type ViewState = ();
    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.with_action_widget(|ctx| {
                let mut pod = ctx.create_pod(TrayButton::new(
                    self.appearance.clone(),
                    self.target.clone(),
                ));
                pod.new_widget.options.disabled = !self.appearance.enabled;
                pod
            }),
            (),
        )
    }
    fn rebuild(
        &self,
        prev: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.target != prev.target {
            element.widget.cancelled_press = true;
            element.widget.target = self.target.clone();
        }
        if self.appearance != prev.appearance {
            let WidgetMut { widget, ctx, .. } = &mut element;
            if let Some(text) = &mut widget.text {
                let mut text = ctx.get_mut(text);
                Label::set_text(&mut text, self.appearance.name.clone());
                if self.appearance.enabled != prev.appearance.enabled {
                    text.ctx.remove_class(if self.appearance.enabled {
                        "shell-muted-shell-ink"
                    } else {
                        "shell-text-shell-ink"
                    });
                    text.ctx.add_class(if self.appearance.enabled {
                        "shell-text-shell-ink"
                    } else {
                        "shell-muted-shell-ink"
                    });
                }
            }
            let focus = self.appearance.selected && !prev.appearance.selected;
            widget.appearance = self.appearance.clone();
            ctx.set_disabled(!self.appearance.enabled);
            ctx.request_layout();
            ctx.request_render();
            if focus && let Some(text) = &mut widget.text {
                ctx.mutate_child_later(text, |mut child| {
                    child
                        .ctx
                        .submit_untyped_action(Box::new(super::tray_widget::Reveal))
                });
            }
        }
    }
    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut ShellState,
    ) -> MessageResult<()> {
        let Some(action) = message.take_message::<ButtonEvent>() else {
            return MessageResult::Stale;
        };
        if action.target != self.target {
            return MessageResult::Action(());
        }
        if let Target::Dock {
            app,
            item,
            menu_path,
        } = &self.target
        {
            super::dock::tray::event(state, &self.output, app, item, menu_path, action.action);
            return MessageResult::Action(());
        }
        if let Target::DesktopContext(generation, submenu, index) = self.target {
            super::desktop::context::button_event(state, generation, submenu, index, action.action);
            return MessageResult::Action(());
        }
        match action.action {
            ButtonAction::MenuAnchor(_) => {}
            ButtonAction::Hover(hovered) => {
                if let Target::Item(key) = &self.target {
                    if hovered {
                        state.tray.hovered = Some((
                            key.clone(),
                            if self.appearance.description.is_empty() {
                                self.appearance.name.clone()
                            } else {
                                self.appearance.description.clone()
                            },
                        ));
                    } else if state
                        .tray
                        .hovered
                        .as_ref()
                        .is_some_and(|(item, _)| item == key)
                    {
                        state.tray.hovered = None;
                    }
                }
            }
            ButtonAction::Dismiss => tray_state::dismiss(state, &self.output),
            ButtonAction::Step(direction) => tray_state::step(state, &self.output, direction),
            ButtonAction::Back => tray_state::back(state, &self.output),
            ButtonAction::Focus => {
                if let Target::Menu(_, _, id) = self.target
                    && let Some(output) = state.outputs.iter_mut().find(|o| o.key() == self.output)
                {
                    output.tray.selected = Some(id);
                }
            }
            ButtonAction::Scroll { delta, horizontal } => {
                if let Target::Item(key) = &self.target {
                    tray_state::send(
                        state,
                        &self.output,
                        TrayCommand::Scroll {
                            key: key.clone(),
                            delta,
                            horizontal,
                        },
                    );
                }
            }
            ButtonAction::Context => {
                if let Target::Item(key) = &self.target {
                    tray_state::open_menu(state, &self.output, key);
                }
            }
            ButtonAction::Secondary => {
                if let Target::Item(key) = &self.target {
                    tray_state::send(
                        state,
                        &self.output,
                        TrayCommand::Secondary {
                            key: key.clone(),
                            x: 0,
                            y: 0,
                        },
                    );
                }
            }
            ButtonAction::Activate => match &self.target {
                Target::Item(key) => tray_state::activate(state, &self.output, key),
                Target::Menu(_, _, id) => tray_state::choose(state, &self.output, *id),
                Target::Back => tray_state::back(state, &self.output),
                Target::Close => tray_state::dismiss(state, &self.output),
                Target::DesktopContext(_, _, _) => {
                    unreachable!("desktop menus are dispatched above")
                }
                Target::Dock { .. } => unreachable!("dock actions are dispatched above"),
            },
        }
        MessageResult::Action(())
    }
}
