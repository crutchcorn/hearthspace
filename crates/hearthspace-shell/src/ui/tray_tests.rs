use super::{
    state::ShellState,
    test_support::{Ui, ids},
    tray_button::{self, Target},
    tray_state, tray_view,
    tray_widget::{Appearance, ButtonAction, ButtonEvent, TrayButton},
};
use crate::tray::{ItemKey, TrayIcon, TrayItem, TrayMenu, TrayMenuItem, TrayStatus, TrayToggle};
use masonry::{
    accesskit::{Role, Toggled},
    core::{
        PointerButton, TextEvent, Widget, WidgetId,
        keyboard::{Key, NamedKey},
    },
    testing::TestHarness,
};
use xilem::{WidgetView, view::sized_box};

fn key(id: u64) -> ItemKey {
    ItemKey {
        owner: format!(":1.{id}"),
        path: "/StatusNotifierItem".into(),
        generation: 1,
    }
}
fn node(id: i32, title: &str) -> TrayMenuItem {
    TrayMenuItem {
        id,
        label: title.into(),
        enabled: true,
        visible: true,
        separator: false,
        submenu: false,
        toggle: None,
        children: Vec::new(),
    }
}
fn item(id: u64) -> TrayItem {
    TrayItem {
        key: key(id),
        title: format!("App {id}"),
        tooltip: "Tray tooltip".into(),
        status: TrayStatus::Active,
        icon: TrayIcon::default(),
        item_is_menu: false,
        menu: Some(TrayMenu {
            items: vec![node(1, "First"), node(2, "Second")],
        }),
        menu_error: None,
        menu_path: Some("/Menu".into()),
    }
}
fn state() -> ShellState {
    let mut state = ShellState::new("/unused-tray".into(), vec!["eDP-1".into(), "DP-1".into()]);
    state.tray.snapshot.items = vec![item(1)];
    state
}
fn panel(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(tray_view::panel(&state.outputs[0], &state.tray))
}
fn name<V: WidgetView<ShellState>>(ui: &Ui<V>, name: &str) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.label().as_deref() == Some(name)
                    && matches!(
                        node.role(),
                        Role::Button
                            | Role::MenuItem
                            | Role::MenuItemCheckBox
                            | Role::MenuItemRadio
                    )
            })
        })
        .unwrap_or_else(|| panic!("missing {name}"))
}
fn press<V: WidgetView<ShellState>>(ui: &mut Ui<V>, key: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
}

#[test]
fn tray_menu_arrows_skip_disabled_hidden_and_separators_and_follow_live_ids() {
    let mut state = state();
    let nodes = &mut state.tray.snapshot.items[0].menu.as_mut().unwrap().items;
    nodes[1].toggle = Some(TrayToggle::Check(true));
    let mut disabled = node(3, "Disabled");
    disabled.enabled = false;
    let mut separator = node(4, "");
    separator.separator = true;
    let mut hidden = node(5, "Hidden");
    hidden.visible = false;
    nodes.splice(1..1, [disabled, separator, hidden]);
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    let mut ui = Ui::new(state, panel, (340, 200));
    assert_eq!(ui.harness.focused_widget_id(), Some(name(&ui, "First")));
    assert!(
        ui.harness
            .access_node(name(&ui, "Disabled"))
            .unwrap()
            .is_disabled()
    );
    assert_eq!(
        ui.harness
            .access_node(name(&ui, "Second"))
            .unwrap()
            .toggled(),
        Some(Toggled::True)
    );
    assert!(ids(&ui.harness).iter().any(|id| {
        ui.harness
            .access_node(*id)
            .is_some_and(|node| node.role() == Role::Splitter)
    }));
    press(&mut ui, NamedKey::ArrowDown);
    ui.dispatch(&panel);
    assert_eq!(ui.state.outputs[0].tray.selected, Some(2));
    assert_eq!(ui.harness.focused_widget_id(), Some(name(&ui, "Second")));
    let nodes = &mut ui.state.tray.snapshot.items[0].menu.as_mut().unwrap().items;
    nodes.reverse();
    tray_state::reconcile(&mut ui.state);
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert_eq!(ui.state.outputs[0].tray.selected, Some(2));
    press(&mut ui, NamedKey::Enter);
    ui.dispatch(&panel);
    assert!(!ui.state.outputs[0].tray.open);
}

#[test]
fn lazy_submenu_stays_open_then_keyboard_back_restores_parent_selection() {
    let mut state = state();
    let submenu = &mut state.tray.snapshot.items[0].menu.as_mut().unwrap().items[0];
    submenu.submenu = true;
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    tray_state::choose(&mut state, "eDP-1", 1);
    tray_state::reconcile(&mut state);
    assert_eq!(state.outputs[0].tray.path, [1]);
    state.tray.snapshot.items[0].menu.as_mut().unwrap().items[0].children = vec![node(8, "Nested")];
    tray_state::reconcile(&mut state);
    let mut ui = Ui::new(state, panel, (340, 180));
    assert_eq!(ui.harness.focused_widget_id(), Some(name(&ui, "Nested")));
    press(&mut ui, NamedKey::ArrowLeft);
    ui.dispatch(&panel);
    assert!(ui.state.outputs[0].tray.path.is_empty());
    assert_eq!(ui.state.outputs[0].tray.selected, Some(1));
    press(&mut ui, NamedKey::Escape);
    ui.dispatch(&panel);
    assert!(!ui.state.outputs[0].tray.open);
}

#[test]
fn removed_items_close_menus_and_panels_remain_exclusive_across_outputs() {
    let mut state = state();
    state.toggle_search("DP-1");
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    assert!(!state.outputs[1].show_search);
    assert!(state.outputs[0].tray.open);
    state.toggle_map("DP-1");
    assert!(!state.outputs[0].tray.open);
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    state.tray.snapshot.items[0].status = TrayStatus::Passive;
    tray_state::reconcile(&mut state);
    assert!(!state.outputs[0].tray.open);
    assert_eq!(tray_state::visible(&state.tray.snapshot).count(), 0);
}

#[test]
fn drawer_makes_menus_touch_reachable_and_shrinks_to_content() {
    let mut state = state();
    tray_state::toggle(&mut state, "eDP-1");
    let mut ui = Ui::new(state, panel, (340, 440));
    ui.harness.mouse_click_on(name(&ui, "Menu for App 1"), None);
    ui.dispatch(&panel);
    assert_eq!(ui.state.outputs[0].tray.item, Some(key(1)));
    let height = super::popup::size(
        &ui.state.outputs[0],
        super::popup::PopupKind::Tray,
        340.0,
        560.0,
    )
    .height;
    assert!(height < 300.0, "{height}");
    let close = name(&ui, "Close");
    ui.harness.mouse_click_on(close, None);
    ui.dispatch(&panel);
    assert!(!ui.state.outputs[0].tray.open);
}

#[test]
fn icon_preserves_input_kinds_and_context_accessibility() {
    let target = Target::Item(key(1));
    let widget = TrayButton::new(
        Appearance {
            name: "App".into(),
            icon: true,
            ..Default::default()
        },
        target.clone(),
    )
    .prepare();
    let mut harness = TestHarness::create_with_size(
        hearthspace_ui_components::hearthspace_light_properties(),
        widget,
        (34, 34),
    );
    let root = harness.root_id();
    for (button, expected) in [
        (Some(PointerButton::Primary), 0),
        (Some(PointerButton::Secondary), 1),
        (Some(PointerButton::Auxiliary), 2),
        (None, 0),
    ] {
        harness.mouse_click_on(root, button);
        let actions = std::iter::from_fn(|| harness.pop_action::<ButtonEvent>())
            .map(|(event, _)| event)
            .collect::<Vec<_>>();
        assert!(actions.iter().any(|event| event.target == target
            && match event.action {
                ButtonAction::Activate => expected == 0,
                ButtonAction::Context => expected == 1,
                ButtonAction::Secondary => expected == 2,
                _ => false,
            }));
    }
    harness.mouse_wheel(masonry::kurbo::Vec2::new(0.0, 120.0));
    assert!(
        std::iter::from_fn(|| harness.pop_action::<ButtonEvent>()).any(|(event, _)| matches!(
            event.action,
            ButtonAction::Scroll {
                delta: 120,
                horizontal: false
            }
        ))
    );
    harness.process_text_event(TextEvent::key_down(Key::Named(NamedKey::ContextMenu)));
    assert!(
        std::iter::from_fn(|| harness.pop_action::<ButtonEvent>())
            .any(|(event, _)| matches!(event.action, ButtonAction::Context))
    );
}

#[test]
fn pointer_release_does_not_activate_a_replacement_tray_item() {
    fn icon(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(tray_button::icon("eDP-1", &state.tray.snapshot.items[0]))
    }
    let mut ui = Ui::new(state(), icon, (34, 34));
    ui.harness.mouse_move_to(name(&ui, "App 1"));
    ui.harness.mouse_button_press(Some(PointerButton::Primary));
    ui.dispatch(&icon);
    ui.state.tray.snapshot.items[0] = item(2);
    ui.rebuild(&icon);
    ui.harness
        .mouse_button_release(Some(PointerButton::Primary));
    ui.dispatch(&icon);
    assert!(
        ui.state.outputs[0].tray.error.is_none(),
        "no unavailable-backend action should be attempted"
    );
}

#[test]
fn queued_activation_cannot_target_a_replacement_icon() {
    fn icon(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(tray_button::icon("eDP-1", &state.tray.snapshot.items[0]))
    }
    let mut ui = Ui::new(state(), icon, (34, 34));
    ui.activate(name(&ui, "App 1"));
    ui.state.tray.snapshot.items[0] = item(2);
    ui.rebuild(&icon);
    ui.dispatch(&icon);
    assert!(ui.state.outputs[0].tray.error.is_none());
}

#[test]
fn overflow_keeps_every_control_visible_and_empty_tray_adds_no_button() {
    fn bar(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(super::views::bar_view(
            &state.outputs[0],
            state.clock.clone(),
            &state.notifications,
            &state.tray,
        ))
    }
    for width in [320, 480, 800] {
        let mut state = state();
        state.tray.snapshot.items = (1..=20).map(item).collect();
        let mut ui = Ui::new(state, bar, (width, 48));
        for label in [
            "Application tray",
            "Search",
            "Notifications",
            "Canvas map",
            "Power",
        ] {
            let bounds = ui
                .harness
                .access_node(name(&ui, label))
                .unwrap()
                .bounding_box()
                .unwrap();
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= width as f64,
                "{width} {label}: {bounds:?}"
            );
        }
        let icons = ids(&ui.harness)
            .into_iter()
            .filter(|id| {
                ui.harness.access_node(*id).is_some_and(|node| {
                    node.role() == Role::Button
                        && node.label().is_some_and(|label| label.starts_with("App "))
                })
            })
            .count();
        assert_eq!(icons, tray_state::capacity(width as f64));
        ui.state.tray.snapshot.items.clear();
        ui.rebuild(&bar);
        ui.dispatch(&bar);
        assert_eq!(
            ids(&ui.harness)
                .into_iter()
                .filter(|id| ui
                    .harness
                    .access_node(*id)
                    .is_some_and(|node| node.role() == Role::Button))
                .count(),
            4
        );
    }
}

#[test]
fn keyboard_navigation_reveals_selected_items_in_a_bounded_menu() {
    let mut state = state();
    state.tray.snapshot.items[0].menu.as_mut().unwrap().items = (1..=30)
        .map(|id| node(id, &format!("Action {id}")))
        .collect();
    tray_state::open_menu(&mut state, "eDP-1", &key(1));
    let mut ui = Ui::new(state, panel, (340, 220));
    for _ in 0..24 {
        press(&mut ui, NamedKey::ArrowDown);
        ui.dispatch(&panel);
    }
    assert_eq!(ui.state.outputs[0].tray.selected, Some(25));
    let selected = name(&ui, "Action 25");
    assert_eq!(ui.harness.focused_widget_id(), Some(selected));
    let bounds = ui
        .harness
        .access_node(selected)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(
        bounds.y0 >= 50.0 && bounds.y1 <= 208.0,
        "selected row should be visible: {bounds:?}"
    );
    let size = super::popup::size(
        &ui.state.outputs[0],
        super::popup::PopupKind::Tray,
        340.0,
        160.0,
    );
    assert_eq!(size.height, 560.0, "long menus remain bounded");
}

#[path = "tray_identity_tests.rs"]
mod identity;

#[path = "tray_live_tests.rs"]
mod live;
