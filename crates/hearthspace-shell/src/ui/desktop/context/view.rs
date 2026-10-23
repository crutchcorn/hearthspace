use super::{Page, ShellState, rows};
use crate::ui::{
    canvas_search::material, popup, popup_group, tray_button, tray_widget::Appearance,
};
use xilem::{
    AnyWidgetView, WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{
        layout::AsUnit,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, button, flex_col, flex_row, label, portal, sized_box},
};

mod name_input;

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    let menu = &state.desktop.menu;
    let context = menu.current.as_ref()?;
    let generation = context.generation;
    Some(popup::window(
        menu.window?,
        "Desktop context menu",
        hearthspace_common::desktop_context::app_id(generation, context.output.as_deref()),
        size(state),
        content(state),
        move |state| super::dismiss(state, generation),
    ))
}

fn size(state: &ShellState) -> LogicalSize<f64> {
    let menu = &state.desktop.menu;
    let context = menu.current.as_ref().expect("active desktop menu");
    let width = if matches!(menu.page, Page::Menu) {
        260.0
    } else {
        340.0_f64
    }
    .min(context.max_width as f64);
    let rows = rows::rows(state);
    let base_height = match &menu.page {
        Page::Name(_) => 160.0,
        Page::Properties(info) => 80.0 + info.len() as f64 * 58.0,
        Page::OpenWith(apps) if apps.is_empty() => 92.0,
        _ => rows.len() as f64 * 34.0 + 12.0,
    };
    let height = (base_height + if menu.error.is_some() { 76.0 } else { 0.0 })
        .min(context.max_height as f64);
    LogicalSize::new(width, height)
}

pub(super) fn content(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let menu = &state.desktop.menu;
    let generation = menu
        .current
        .as_ref()
        .expect("active desktop menu")
        .generation;
    let LogicalSize { width, height } = size(state);
    let rows = rows::rows(state);
    let content: Box<AnyWidgetView<ShellState>> = match &menu.page {
        Page::Name(folder) => name_form(state, *folder, generation).boxed(),
        _ => {
            let mut views: Vec<Box<AnyWidgetView<ShellState>>> = Vec::new();
            if let Page::Properties(info) = &menu.page {
                for (name, value) in info {
                    views.push(
                        sized_box(
                            flex_col((
                                label(name.clone())
                                    .text_size(11.0)
                                    .color(state.appearance.palette.text_muted),
                                label(value.clone())
                                    .text_size(13.0)
                                    .color(state.appearance.palette.text)
                                    .line_break_mode(LineBreaking::WordWrap),
                            ))
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .gap(4.0.px()),
                        )
                        .padding(10.0.px())
                        .boxed(),
                    );
                }
            }
            if matches!(&menu.page, Page::OpenWith(apps) if apps.is_empty()) {
                views.push(
                    sized_box(
                        label("No applications are associated with this file type.")
                            .text_size(13.0)
                            .line_break_mode(LineBreaking::WordWrap),
                    )
                    .padding(10.0.px())
                    .boxed(),
                );
            }
            views.extend(rows.into_iter().enumerate().map(|(index, row)| {
                let close = matches!(row.action, rows::Choice::Close);
                let selected = if menu.busy {
                    close
                } else {
                    menu.selected == index
                };
                tray_button::ButtonView {
                    output: String::new(),
                    target: tray_button::Target::DesktopContext(generation, index),
                    appearance: Appearance {
                        name: row.name,
                        menu: true,
                        selected,
                        autofocus: selected,
                        enabled: !menu.busy || close,
                        submenu: row.submenu,
                        ..Default::default()
                    },
                }
                .boxed()
            }));
            popup_group::group(
                "Desktop context menu",
                flex_col(views)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(0.0.px()),
            )
            .menu()
            .boxed()
        }
    };
    let error = menu.error.as_ref().map(|error| {
        popup_group::group(
            "Desktop action",
            sized_box(
                label(error.clone())
                    .text_size(12.0)
                    .line_break_mode(LineBreaking::WordWrap),
            )
            .padding(10.0.px()),
        )
        .status()
    });
    let panel = material::shell(
        sized_box(
            portal(
                flex_col((content, error))
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .gap(4.0.px()),
            )
            .constrain_horizontal(true)
            .must_fill(false),
        )
        .padding(6.0.px())
        .width(width.px())
        .height(height.px()),
        state.appearance.palette,
    );
    crate::ui::search_focus::focus_on_activation(panel, Default::default(), generation.to_string())
        .with_dismiss(|state, key| {
            if let Ok(generation) = key.parse() {
                super::dismiss(state, generation);
            }
        })
        .dismiss_directly()
}

fn name_form(
    state: &ShellState,
    folder: bool,
    generation: u64,
) -> impl WidgetView<ShellState> + use<> {
    let target = state.desktop.menu.focus.clone();
    let input = name_input::Input {
        text: state.desktop.menu.text.clone(),
        target: target.clone(),
        folder,
    }
    .height(38.0.px())
    .padding(8.0.px());
    let palette = state.appearance.palette;
    let cancel = hearthspace_ui_components::button_surface(
        button(label("Cancel"), move |state: &mut ShellState| {
            super::dismiss(state, generation)
        })
        .padding(Padding::from_vh(8.0.px(), 12.0.px())),
        hearthspace_ui_components::ButtonSurface::Control,
        false,
    );
    let submit = hearthspace_ui_components::button_surface(
        button(
            label(if folder { "Create Folder" } else { "Rename" }),
            move |state: &mut ShellState| name_input::submit(state, folder),
        )
        .padding(Padding::from_vh(8.0.px(), 12.0.px())),
        hearthspace_ui_components::ButtonSurface::Control,
        true,
    );
    let form = popup_group::group(
        if folder { "New Folder" } else { "Rename" },
        sized_box(
            flex_col((
                label(if folder { "New Folder" } else { "Rename" }).text_size(14.0),
                material::panel(input, false, palette),
                flex_row((cancel, submit)).gap(8.0.px()),
            ))
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .gap(12.0.px()),
        )
        .padding(10.0.px()),
    )
    .dialog();
    crate::ui::search_focus::focus_on_activation(form, target, generation.to_string())
        .with_dismiss(|state, key| {
            if let Ok(generation) = key.parse() {
                super::dismiss(state, generation);
            }
        })
        .dismiss_directly()
}
