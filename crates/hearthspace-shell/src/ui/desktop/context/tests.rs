use super::*;
use crate::{
    desktop_items::{DesktopItem, DesktopItemKind},
    ui::test_support::{Ui, ids},
};
use masonry::{
    accesskit::Role,
    core::{
        Ime, TextEvent, WidgetId,
        keyboard::{Key, NamedKey},
    },
};
use xilem::{
    WidgetView,
    view::{flex_col, sized_box},
};

fn state(file: bool, height: i32) -> (ShellState, tokio::sync::mpsc::Receiver<task::Request>) {
    let mut state = ShellState::new("/unused".into(), vec!["eDP-1".into()]);
    state.desktop.replace_items(vec![DesktopItem {
        id: "file:test".into(),
        title: "Song.wav".into(),
        kind: DesktopItemKind::File {
            path: "/owned/Song.wav".into(),
        },
    }]);
    let (sender, receiver) = tokio::sync::mpsc::channel(32);
    state.desktop.requests = Some(sender);
    apply(
        &mut state,
        Some(DesktopContext {
            generation: 8,
            ids: if file {
                vec!["file:test".into()]
            } else {
                vec![]
            },
            output: Some("eDP-1".into()),
            max_width: 340,
            max_height: height,
        }),
    );
    (state, receiver)
}

fn panel(
    state: &ShellState,
) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
    sized_box(flex_col((state
        .desktop
        .menu
        .current
        .as_ref()
        .map(|_| view::content(state)),)))
}
fn named<V: WidgetView<ShellState>>(ui: &Ui<V>, name: &str, role: Role) -> WidgetId
where
    V::Widget: Sized,
{
    ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness
                .access_node(*id)
                .is_some_and(|node| node.label().as_deref() == Some(name) && node.role() == role)
        })
        .unwrap_or_else(|| panic!("Missing {role:?}: {name}"))
}
fn press<V: WidgetView<ShellState>>(ui: &mut Ui<V>, key: NamedKey)
where
    V::Widget: Sized,
{
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(key)));
}

#[test]
fn file_context_rename_exposes_a_focused_editor_and_reports_worker_failure() {
    let (state, mut requests) = state(true, 600);
    let mut ui = Ui::new(state, panel, (340, 320));
    named(&ui, "Desktop context menu", Role::Menu);
    let rename = named(&ui, "Rename…", Role::MenuItem);
    ui.activate(rename);
    ui.dispatch(&panel);
    let editor = named(&ui, "Name", Role::TextInput);
    assert_eq!(ui.harness.focused_widget_id(), Some(editor));
    ui.harness
        .process_text_event(TextEvent::Ime(Ime::Commit("2".into())));
    ui.dispatch(&panel);
    let edited = ui.state.desktop.menu.text.clone();
    assert!(edited.contains('2'));
    press(&mut ui, NamedKey::Enter);
    ui.dispatch(&panel);
    let task::Request::Context(request) = requests.try_recv().unwrap() else {
        panic!("missing rename request");
    };
    assert_eq!(request.generation, 8);
    assert!(matches!(request.action, Action::Rename(name) if name == edited));
    assert!(ui.state.desktop.menu.busy);
    finished(
        &mut ui.state,
        8,
        Err("A file with this name already exists.".into()),
    );
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    named(&ui, "Desktop action", Role::Status);
    assert_eq!(ui.state.desktop.menu.text, edited);
    press(&mut ui, NamedKey::Escape);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.current.is_none());
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command) if command == "desktop-context-close 8")
    );
}

#[test]
fn canvas_context_menu_keyboard_scrolls_to_actions_in_a_small_output() {
    let (state, mut requests) = state(false, 180);
    let mut ui = Ui::new(state, panel, (260, 180));
    assert_eq!(
        ui.harness.focused_widget_id(),
        Some(named(&ui, "New Folder…", Role::MenuItem))
    );
    for _ in 0..7 {
        press(&mut ui, NamedKey::ArrowDown);
        ui.dispatch(&panel);
    }
    let settings = named(&ui, "Settings", Role::MenuItem);
    assert_eq!(ui.harness.focused_widget_id(), Some(settings));
    let bounds = ui
        .harness
        .access_node(settings)
        .unwrap()
        .bounding_box()
        .unwrap();
    assert!(bounds.y0 >= 0.0 && bounds.y1 <= 180.0, "{bounds:?}");
    press(&mut ui, NamedKey::Enter);
    ui.dispatch(&panel);
    assert!(requests.try_recv().is_err());
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    ui.dispatch(&panel);
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            generation: 8,
            action: Action::Settings
        })
    ));
}

#[test]
fn replaced_menu_ignores_late_file_action_results_and_stale_clicks() {
    let (mut state, mut requests) = state(true, 600);
    let mut next = state.desktop.menu.current.clone().unwrap();
    next.generation = 9;
    apply(&mut state, Some(next));
    finished(&mut state, 8, Ok(Completed::Closed(None)));
    button_event(&mut state, 8, false, 0, ButtonAction::Activate);
    assert_eq!(state.desktop.menu.current.as_ref().unwrap().generation, 9);
    assert!(requests.try_recv().is_err());
}

#[test]
fn a_full_action_queue_preserves_the_menu_and_clears_busy_state() {
    let (mut state, _requests) = state(false, 600);
    for _ in 0..32 {
        task::submit(&mut state, task::Request::Command("desktop-clear".into()));
    }
    send(&mut state, Action::Paste);
    assert!(!state.desktop.menu.busy);
    assert!(state.desktop.menu.current.is_some());
    assert!(
        state
            .desktop
            .menu
            .error
            .as_deref()
            .unwrap()
            .contains("busy")
    );
}

#[test]
fn busy_menu_escape_dismissal_survives_a_snapshot_while_transfer_finishes() {
    let (state, mut requests) = state(true, 600);
    let mut ui = Ui::new(state, panel, (340, 320));
    let context = ui.state.desktop.menu.current.clone();
    let copy = named(&ui, "Copy", Role::MenuItem);
    ui.activate(copy);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.busy);
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(_)
    ));
    press(&mut ui, NamedKey::Escape);
    ui.dispatch(&panel);
    assert!(ui.state.desktop.menu.current.is_none());
    apply(&mut ui.state, context);
    assert!(ui.state.desktop.menu.current.is_none());
    finished(&mut ui.state, 8, Ok(Completed::Closed(None)));
    assert!(ui.state.desktop.menu.current.is_none());
}

#[test]
fn open_with_keeps_the_parent_menu_and_returns_from_a_separate_child() {
    let (state, mut requests) = state(true, 600);
    let mut ui = Ui::new(state, panel, (340, 320));
    let open_with = named(&ui, "Open With…", Role::MenuItem);
    let original_window = ui.state.desktop.menu.window;
    ui.activate(open_with);
    ui.dispatch(&panel);
    assert_eq!(windows(&ui.state).len(), 2);
    assert_eq!(ui.state.desktop.menu.window, original_window);
    assert_eq!(named(&ui, "Open With…", Role::MenuItem), open_with);
    named(&ui, "Properties", Role::MenuItem);
    assert!(matches!(
        requests.try_recv().unwrap(),
        task::Request::Context(context_actions::Request {
            action: Action::OpenWithOptions,
            ..
        })
    ));
    finished(
        &mut ui.state,
        8,
        Ok(Completed::OpenWith(vec![
            crate::desktop_items::actions::OpenWithApp {
                id: "music.desktop".into(),
                name: "Music".into(),
            },
            crate::desktop_items::actions::OpenWithApp {
                id: "player.desktop".into(),
                name: "Player".into(),
            },
        ])),
    );
    ui.rebuild(&panel);
    ui.dispatch(&panel);
    assert_eq!(
        ui.harness
            .access_node(open_with)
            .unwrap()
            .data()
            .is_expanded(),
        Some(true)
    );
    fn child(
        state: &ShellState,
    ) -> impl WidgetView<ShellState, Widget = masonry::widgets::SizedBox> + use<> {
        sized_box(view::panel(state, true))
    }
    let mut submenu = Ui::new(ui.state, child, (340, 160));
    assert_eq!(
        submenu.harness.focused_widget_id(),
        Some(named(&submenu, "Music", Role::MenuItem))
    );
    press(&mut submenu, NamedKey::ArrowDown);
    submenu.dispatch(&child);
    assert_eq!(
        submenu.harness.focused_widget_id(),
        Some(named(&submenu, "Player", Role::MenuItem))
    );
    press(&mut submenu, NamedKey::ArrowLeft);
    submenu.dispatch(&child);
    assert!(submenu.state.desktop.menu.submenu.is_none());
    assert_eq!(submenu.state.desktop.menu.selected, 1);
    assert_eq!(submenu.state.desktop.menu.window, original_window);
    assert_eq!(windows(&submenu.state).len(), 1);
    assert!(
        matches!(requests.try_recv().unwrap(), task::Request::Command(command) if command == "desktop-context-parent 8")
    );
}

#[test]
fn desktop_submenu_back_while_loading_does_not_reopen_on_late_completion() {
    let (mut state, mut requests) = state(true, 600);
    button_event(&mut state, 8, false, 1, ButtonAction::Activate);
    assert!(state.desktop.menu.submenu.is_some());
    requests.try_recv().unwrap();
    button_event(&mut state, 8, true, 0, ButtonAction::Back);
    assert!(state.desktop.menu.submenu.is_none());
    finished(&mut state, 8, Ok(Completed::OpenWith(vec![])));
    assert_eq!(windows(&state).len(), 1);
    assert!(!state.desktop.menu.busy);
    assert!(state.desktop.menu.current.is_some());
}

#[test]
fn menu_label_is_vertically_centered_inside_its_interactive_row() {
    let (state, _) = state(false, 600);
    let ui = Ui::new(state, panel, (260, 300));
    let row = ui
        .harness
        .get_widget_with_id(named(&ui, "Settings", Role::MenuItem));
    let bounds = row.ctx().bounding_box();
    let label = row.children().into_iter().next().unwrap();
    let text = label.ctx().bounding_box();
    assert!(text.height() < bounds.height());
    assert!(
        (text.center().y - bounds.center().y).abs() < 0.5,
        "row {bounds:?}, label {text:?}"
    );
}

mod gpu;
mod interaction;
