use super::*;
use hearthspace_common::{AccentColor, Appearance, ColorMode};
use hearthspace_ui_components::Palette;

#[test]
fn graphics_preference_wraps_and_remains_keyboard_operable_in_both_themes() {
    let directory = tempfile::tempdir().unwrap();
    for (theme, color_mode) in [("light", ColorMode::Light), ("dark", ColorMode::Dark)] {
        for (width, height) in [(340, 320), (240, 180)] {
            let (mut state, mut requests) = app_state(directory.path(), false);
            let context = state.desktop.menu.current.as_mut().unwrap();
            context.max_width = width;
            context.max_height = height;
            state.appearance.update(Palette::new(Appearance {
                color_mode,
                accent_color: AccentColor::Blue,
            }));
            let mut ui = Ui::new(state, panel, (width as u32, height as u32));
            let checkbox = named(&ui, LABEL, Role::MenuItemCheckBox);
            for _ in 0..8 {
                if ui.harness.focused_widget_id() == Some(checkbox) {
                    break;
                }
                press(&mut ui, NamedKey::ArrowDown);
                ui.dispatch(&panel);
            }
            assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
            let row = ui.harness.get_widget_with_id(checkbox);
            let bounds = row.ctx().bounding_box();
            assert!(
                bounds.x0 >= 0.0 && bounds.x1 <= f64::from(width),
                "{bounds:?}"
            );
            assert!(
                bounds.y0 >= 0.0 && bounds.y1 <= f64::from(height),
                "{bounds:?}"
            );
            let label = row.children().into_iter().next().unwrap();
            let text = label.ctx().bounding_box();
            assert!(
                text.x0 >= bounds.x0 && text.x1 <= bounds.x1,
                "{text:?}, {bounds:?}"
            );
            if width == 240 {
                assert!(
                    text.height() > 20.0,
                    "narrow checkbox label must wrap: {text:?}"
                );
            }
            let image = ui.harness.render();
            // Opt-in artifacts use the same production widget tree exercised
            // above; ordinary tests remain independent of an evidence directory.
            if let Some(path) = std::env::var_os("HEARTHSPACE_TEST_MENU_CAPTURES") {
                let path = std::path::PathBuf::from(path);
                image
                    .save(path.join(format!("gpu-menu-{theme}-{width}.png")))
                    .unwrap();
            }
            ui.harness
                .process_text_event(TextEvent::key_down(Key::Character(" ".into())));
            ui.dispatch(&panel);
            assert!(requests.try_recv().is_err());
            ui.harness
                .process_text_event(TextEvent::key_up(Key::Character(" ".into())));
            ui.dispatch(&panel);
            assert!(matches!(
                requests.try_recv().unwrap(),
                task::Request::Context(context_actions::Request {
                    action: Action::GpuPreference(true),
                    ..
                })
            ));
            finished(
                &mut ui.state,
                8,
                Err("The application preference could not be saved. Try again.".into()),
            );
            ui.rebuild(&panel);
            ui.dispatch(&panel);
            assert_eq!(ui.harness.focused_widget_id(), Some(checkbox));
            assert_eq!(
                ui.harness.access_node(checkbox).unwrap().toggled(),
                Some(Toggled::False)
            );
            named(&ui, "Desktop action", Role::Status);
            assert!(requests.try_recv().is_err());
        }
    }
}
