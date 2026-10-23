use super::*;

#[test]
fn stopped_worker_restores_tray_instead_of_hiding_an_unavailable_menu() {
    let dir = tempfile::tempdir().unwrap();
    let (state, worker) = fixture(dir.path());
    assert!(!fallback(&state, "eDP-1"));
    drop(worker);
    assert!(fallback(&state, "eDP-1"));
}

#[test]
fn missing_menu_or_failed_menu_keeps_original_tray_action_reachable() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.tray.snapshot.items[0].menu_error = Some("Menu unavailable".into());
    assert!(fallback(&state, "eDP-1"));
    state.tray.snapshot.items[0].menu_error = None;
    state.tray.snapshot.items[0].menu_path = None;
    assert!(fallback(&state, "eDP-1"));
    state.tray.snapshot.items[0].menu_path = Some("/Menu".into());
    state.tray.snapshot.items[0].menu = None;
    assert!(fallback(&state, "eDP-1"));
    let mut ui = Ui::new(state, view, (500, 300));
    ui.harness.mouse_click_on(
        ui.button("Localized status title"),
        Some(PointerButton::Secondary),
    );
    ui.dispatch(&view);
    assert!(
        !ui.state.outputs[0].tray.open,
        "legacy ContextMenu is sent instead of losing the action"
    );
    assert!(ui.state.outputs[0].tray.error.is_none());
}

#[test]
fn ambiguous_catalog_or_multiple_status_items_never_discard_a_menu() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let mut second = state.tray.snapshot.items[0].clone();
    second.key.path = "/SecondItem".into();
    state.tray.snapshot.items.push(second);
    assert_eq!(tray_state::strip_visible(&state, "eDP-1").count(), 2);
    state.tray.snapshot.items.pop();
    install(dir.path(), "other_steam", "X-SnapAppName=steam\n");
    state.catalog = AppCatalog::load_from_data_dirs(vec![dir.path().to_owned()]);
    assert!(fallback(&state, "eDP-1"));
}

#[test]
fn localized_titles_icons_bus_names_and_executable_guesses_do_not_match() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.tray.snapshot.items[0].id = "unknown".into();
    state.tray.snapshot.items[0].title = "Steam".into();
    state.tray.snapshot.items[0].icon.name = Some("steam".into());
    state.tray.snapshot.items[0].key.owner = "org.steam".into();
    assert_eq!(tray_state::strip_visible(&state, "eDP-1").count(), 1);
    state.tray.snapshot.items[0].id = "unused".into();
    assert!(state.catalog.app_for_window("unused").is_some());
    assert!(state.catalog.app_for_status_item("unused").is_none());
    assert_eq!(tray_state::strip_visible(&state, "eDP-1").count(), 1);
}

#[test]
fn suppression_is_per_output_page_and_tracks_hidden_compact_and_closed_apps() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    state.frame_layout.snapshot.outputs[1]
        .layout
        .set_placement(&LayoutTarget::widget("apps"), ObjectPlacement::Hidden);
    assert!(!fallback(&state, "eDP-1"));
    assert!(fallback(&state, "DP-1"));
    state.frame_layout.snapshot.outputs[0].widgets[0].rect.width = 98.0;
    assert!(
        fallback(&state, "eDP-1"),
        "compact cycle control cannot own a menu"
    );
    state.frame_layout.snapshot.outputs[0].widgets[0].rect.width = 126.0;
    state.windows.push(RunningWindow {
        id: 8,
        app_id: "aaa".into(),
        title: "Other".into(),
        focused: false,
    });
    assert!(fallback(&state, "eDP-1"), "Steam is on the next page");
    state.outputs[0].dock_page = 1;
    assert!(!fallback(&state, "eDP-1"));
    state.windows.clear();
    assert!(fallback(&state, "eDP-1"));
    state.favorites.store.add("steam_steam.desktop").unwrap();
    assert!(
        !fallback(&state, "eDP-1"),
        "a visible closed pin still exposes its menu"
    );
}

#[test]
fn horizontal_vertical_and_grouped_docks_share_the_visible_menu_owner() {
    let dir = tempfile::tempdir().unwrap();
    let (mut state, _worker) = fixture(dir.path());
    let output = &mut state.frame_layout.snapshot.outputs[0];
    output.layout.groups.insert(
        "dock".into(),
        FrameGroup {
            id: "dock".into(),
            members: vec![LayoutTarget::widget("apps")],
        },
    );
    output.widgets[0].rect.width = 92.0;
    assert!(!fallback(&state, "eDP-1"));
    let output = &mut state.frame_layout.snapshot.outputs[0];
    output.layout.set_placement(
        &LayoutTarget::widget("apps"),
        ObjectPlacement::frame(FrameEdge::Left, 0.5),
    );
    output.widgets[0].orientation = "vertical".into();
    output.widgets[0].rect = Rect {
        width: 54.0,
        height: 102.0,
        ..Default::default()
    };
    assert!(!fallback(&state, "eDP-1"));
    state.frame_layout.snapshot.outputs[0].widgets[0]
        .rect
        .height = 80.0;
    assert!(fallback(&state, "eDP-1"));
}
