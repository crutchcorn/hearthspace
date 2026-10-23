//! Use desktop-file names long enough to exercise the actual installed catalog.
use super::*;

fn long_catalog() -> ShellState {
    let directory = tempfile::tempdir().unwrap();
    let applications = directory.path().join("applications");
    fs::create_dir(&applications).unwrap();
    for (index, name) in [
        "Advanced Network Configuration",
        "Calculator",
        "Characters",
        "ChatGPT",
        "Disk Usage Analyzer",
        "Document Viewer",
        "Hearthspace Settings",
        "ImageMagick (color depth=q16)",
        "Visual Studio Code",
        "VeryLongApplicationNameWithoutAnyWhitespaceOrWordBreaks",
    ]
    .into_iter()
    .cycle()
    .take(32)
    .enumerate()
    {
        fs::write(
            applications.join(format!("long-{index:02}.desktop")),
            format!(
                "[Desktop Entry]\nType=Application\nName={name} {index:02}\n\
                 Comment=Configure applications, networks, connections, devices and services for this computer\nExec=true\n"
            ),
        )
        .unwrap();
    }
    let mut state = state();
    state.catalog = AppCatalog::load_from_data_dirs(vec![directory.path().into()]);
    state.change_query("eDP-1", String::new());
    reconcile(&mut state);
    state
}

fn bounds(
    ui: &Ui<impl WidgetView<ShellState, Widget: Sized>>,
    name: &str,
) -> masonry::accesskit::Rect {
    let id = ids(&ui.harness)
        .into_iter()
        .find(|id| {
            ui.harness.access_node(*id).is_some_and(|node| {
                node.label().as_deref() == Some(name) || node.value().as_deref() == Some(name)
            })
        })
        .unwrap_or_else(|| panic!("Missing accessible content: {name}"));
    ui.harness.access_node(id).unwrap().bounding_box().unwrap()
}

#[test]
fn long_names_keep_layout_controls_and_all_columns_inside_the_popup() {
    for width in [650, 272] {
        let mut ui = Ui::new(long_catalog(), view, (width, 560));
        let columns = launcher::grid_columns(&ui.state.outputs[0]);
        let entries = launcher::results(&ui.state, &ui.state.outputs[0]);
        for name in ["Close apps list", "Show app grid", "Show app list"] {
            let control = bounds(&ui, name);
            assert!(
                control.x0 >= 0.0 && control.x1 <= f64::from(width),
                "{name}: {control:?}"
            );
        }
        let first = bounds(&ui, &entries[0].label());
        for entry in entries.iter().take(columns) {
            let tile = bounds(&ui, &entry.label());
            assert_eq!(tile.y0, first.y0);
            assert!(tile.x0 >= 0.0 && tile.x1 <= f64::from(width), "{tile:?}");
            assert!(
                tile.width() <= f64::from(width) / columns as f64,
                "{tile:?}"
            );
            let launcher::Entry::App(app) = entry else {
                unreachable!()
            };
            let title = bounds(&ui, &app.name);
            assert!(
                title.x0 >= tile.x0 && title.x1 <= tile.x1,
                "{title:?} in {tile:?}"
            );
        }
        ui.harness.mouse_click_on(ui.button("Show app list"), None);
        ui.dispatch(&view);
        assert_eq!(
            ui.state.outputs[0].launcher_layout,
            launcher::AppLayout::List
        );
        for entry in entries.iter().take(4) {
            let row = bounds(&ui, &entry.label());
            assert!(row.x0 >= 0.0 && row.x1 <= f64::from(width), "{row:?}");
            let launcher::Entry::App(app) = entry else {
                unreachable!()
            };
            let title = bounds(&ui, &app.name);
            assert!(
                title.x0 >= row.x0 && title.x1 <= row.x1,
                "{title:?} in {row:?}"
            );
        }
        ui.state.change_query(
            "eDP-1",
            "VeryLongApplicationNameWithoutAnyWhitespaceOrWordBreaks".into(),
        );
        ui.rebuild(&view);
        ui.dispatch(&view);
        for name in ["Clear search", "Draw to launch", "Close apps list"] {
            let control = bounds(&ui, name);
            assert!(
                control.x0 >= 0.0 && control.x1 <= f64::from(width),
                "{name}: {control:?}"
            );
        }
    }
}

#[test]
fn long_catalog_keeps_footer_and_layout_controls_visible_while_results_scroll() {
    let mut ui = Ui::new(long_catalog(), view, (650, 560));
    let footer = bounds(&ui, "Enter to open");
    let toggle = bounds(&ui, "Show app list");
    assert!(footer.y0 > 480.0 && footer.y1 <= 560.0, "{footer:?}");
    let last = launcher::results(&ui.state, &ui.state.outputs[0])
        .last()
        .unwrap()
        .label();
    for _ in 0..8 {
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::ArrowDown)));
        ui.dispatch(&view);
    }
    assert_eq!(selected(&ui), std::slice::from_ref(&last));
    let selected = bounds(&ui, &last);
    assert!(
        selected.y0 >= toggle.y1 && selected.y1 <= footer.y0,
        "{selected:?}"
    );
    assert_eq!(bounds(&ui, "Enter to open"), footer);
    assert_eq!(bounds(&ui, "Show app list"), toggle);
}
