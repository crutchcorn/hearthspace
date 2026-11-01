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

pub(super) fn windows(state: &ShellState) -> Vec<WindowView<ShellState>> {
    let menu = &state.desktop.menu;
    let Some(context) = &menu.current else {
        return vec![];
    };
    let generation = context.generation;
    let mut windows = vec![popup::window(
        menu.window.expect("active context window"),
        "Desktop context menu",
        hearthspace_common::desktop_context::app_id(generation, context.output.as_deref()),
        size(state, false),
        content(state),
        move |state| super::dismiss(state, generation),
    )];
    if let Some(submenu) = &menu.submenu {
        windows.push(popup::window(
            submenu.window,
            "Open With",
            hearthspace_common::desktop_context::submenu_app_id(
                generation,
                context.output.as_deref(),
                submenu.row_y,
            ),
            size(state, true),
            panel(state, true),
            move |state| super::back(state, generation),
        ));
    }
    windows
}

fn size(state: &ShellState, submenu: bool) -> LogicalSize<f64> {
    let menu = &state.desktop.menu;
    let context = menu.current.as_ref().expect("active desktop menu");
    let gpu = !submenu && rows::gpu_preference(state).is_some();
    let width = if !submenu && matches!(menu.page, Page::Menu) && !gpu {
        260.0_f64
    } else {
        340.0
    }
    .min(context.max_width as f64);
    let count = rows::rows(state, submenu).len();
    let base_height = if !submenu && matches!(menu.page, Page::Name(_)) {
        160.0
    } else if submenu
        && menu
            .submenu
            .as_ref()
            .is_some_and(|child| child.apps.as_ref().is_none_or(|apps| apps.is_empty()))
    {
        112.0
    } else {
        count as f64 * 34.0 + 12.0
    };
    let error = menu.error.is_some() && (submenu || menu.submenu.is_none());
    let height = (base_height + if gpu { 72.0 } else { 0.0 } + if error { 76.0 } else { 0.0 })
        .min(context.max_height as f64);
    LogicalSize::new(width, height)
}

pub(super) fn content(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    panel(state, false)
}

pub(super) fn panel(state: &ShellState, submenu: bool) -> impl WidgetView<ShellState> + use<> {
    let menu = &state.desktop.menu;
    let generation = menu
        .current
        .as_ref()
        .expect("active desktop menu")
        .generation;
    let LogicalSize { width, height } = size(state, submenu);
    let content: Box<AnyWidgetView<ShellState>> = if !submenu && let Page::Name(folder) = &menu.page
    {
        name_form(state, *folder, generation).boxed()
    } else {
        let mut views: Vec<Box<AnyWidgetView<ShellState>>> = Vec::new();
        if submenu
            && let Some(child) = &menu.submenu
            && child.apps.as_ref().is_none_or(|apps| apps.is_empty())
        {
            views.push(
                sized_box(
                    label(if child.apps.is_none() && menu.busy {
                        "Loading applications…"
                    } else {
                        "No applications are associated with this file type."
                    })
                    .text_size(13.0)
                    .line_break_mode(LineBreaking::WordWrap),
                )
                .padding(10.0.px())
                .boxed(),
            );
        }
        views.extend(
            rows::rows(state, submenu)
                .into_iter()
                .enumerate()
                .map(|(index, row)| {
                    let close = matches!(row.action, rows::Choice::Close | rows::Choice::Back);
                    let selected_index = if submenu {
                        menu.submenu.as_ref().map_or(0, |child| child.selected)
                    } else {
                        menu.selected
                    };
                    let selected = if menu.busy {
                        close
                    } else {
                        selected_index == index
                    };
                    let expanded = row.submenu.then_some(menu.submenu.is_some());
                    tray_button::ButtonView {
                        output: String::new(),
                        target: tray_button::Target::DesktopContext(generation, submenu, index),
                        appearance: Appearance {
                            name: row.name,
                            description: if row.checked.is_some() {
                                rows::GPU_HELP.into()
                            } else {
                                String::new()
                            },
                            toggle: row.checked.map(crate::tray::TrayToggle::Check),
                            wrap_label: row.checked.is_some(),
                            menu: true,
                            selected,
                            autofocus: selected,
                            enabled: !menu.busy || close,
                            activate_on_key_release: true,
                            submenu: row.submenu,
                            expanded,
                            ..Default::default()
                        },
                    }
                    .boxed()
                }),
        );
        popup_group::group(
            if submenu {
                "Open With"
            } else {
                "Desktop context menu"
            },
            flex_col(views)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .gap(0.0.px()),
        )
        .menu()
        .boxed()
    };
    let error = menu
        .error
        .as_ref()
        .filter(|_| submenu || menu.submenu.is_none())
        .map(|error| {
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
    let gpu_help = (!submenu && rows::gpu_preference(state).is_some()).then(|| {
        let message = if menu.saving_gpu {
            format!("Saving graphics preference… {}", rows::GPU_HELP)
        } else if menu.saved_gpu {
            format!("Saved. {}", rows::GPU_HELP)
        } else {
            rows::GPU_HELP.into()
        };
        popup_group::group(
            "Graphics preference",
            sized_box(
                label(message)
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
                flex_col((content, gpu_help, error))
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
    crate::ui::search_focus::focus_on_activation(
        panel,
        Default::default(),
        format!("{generation}:{}", u8::from(submenu)),
    )
    .with_dismiss(|state, key| {
        if let Some((generation, child)) = key.split_once(':')
            && let Ok(generation) = generation.parse()
        {
            if child == "1" {
                super::back(state, generation);
            } else {
                super::dismiss(state, generation);
            }
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
