//! A session-bound native dialog; only its explicit affirmative action mutates.
use crate::ui::{search_focus::Record, state::ShellState};
use hearthspace_ipc::frame_layout::{Action, Confirmation};
use hearthspace_ui_components as components;
use xilem::{
    WidgetView, WindowView,
    dpi::LogicalSize,
    masonry::{
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{
        CrossAxisAlignment, MainAxisAlignment, button, flex_col, flex_row, label, portal,
        resize_observer, sized_box,
    },
};

pub(super) fn window(state: &ShellState) -> Option<WindowView<ShellState>> {
    let layout = &state.frame_layout;
    let confirmation = layout.snapshot.confirmation.as_ref()?;
    let width = layout.snapshot.confirmation_max_width.clamp(120.0, 420.0);
    let height = layout
        .confirm_height
        .unwrap_or(if matches!(confirmation, Confirmation::HideTools) {
            300.0
        } else {
            220.0
        })
        .min(layout.snapshot.confirmation_max_height.max(100.0));
    Some(crate::ui::popup::window(
        layout.confirm_window?,
        "Confirm layout change",
        hearthspace_common::frame_layout::layout_confirm_app_id(
            layout.snapshot.session,
            layout.snapshot.output.as_deref(),
        ),
        LogicalSize::new(width, height),
        view(state, width, height),
        |state| super::command(state, Action::Cancel),
    ))
}

fn dismiss(state: &mut ShellState, _: &str) {
    super::command(state, Action::Cancel);
}

pub(super) fn view(
    state: &ShellState,
    width: f64,
    height: f64,
) -> impl WidgetView<ShellState> + use<> {
    let tools = state
        .frame_layout
        .snapshot
        .confirmation
        .as_ref()
        .is_some_and(|confirmation| matches!(confirmation, Confirmation::HideTools));
    let (title, body, affirm) = if tools {
        (
            "Keep a way back to your canvas",
            "Canvas tools can be removed after you enable “Make canvas tools hide-able” in Settings. You can find them again with ❖ + K search, or restore them from Settings.",
            "Open settings",
        )
    } else {
        (
            "Close and remove from pinned?",
            "This closes the window and removes it from the frame.",
            "Close and remove",
        )
    };
    let target = state.frame_layout.confirm_target.clone();
    let cancel = Record {
        inner: components::button_surface(
            button(
                label("Cancel")
                    .font("Liberation Sans")
                    .text_size(13.0)
                    .color(state.appearance.palette.text),
                |state: &mut ShellState| super::command(state, Action::Cancel),
            )
            .padding(Padding::from_vh(10.0.px(), 14.0.px()))
            .border_width(0.0.px())
            .corner_radius(10.0.px()),
            components::ButtonSurface::Control,
            false,
        ),
        target: target.clone(),
    };
    let affirmative = components::button_surface(
        button(
            label(affirm)
                .font("Liberation Sans")
                .text_size(13.0)
                .color(if tools {
                    state.appearance.palette.text
                } else {
                    xilem::Color::WHITE
                }),
            |state: &mut ShellState| super::command(state, Action::Confirm),
        )
        .padding(Padding::from_vh(10.0.px(), 14.0.px()))
        .border_width(0.0.px())
        .corner_radius(10.0.px()),
        if tools {
            components::ButtonSurface::Control
        } else {
            components::ButtonSurface::Danger
        },
        false,
    );
    let content = flex_col((
        label(title)
            .font("Liberation Sans")
            .text_size(19.0)
            .weight(FontWeight::BOLD)
            .color(state.appearance.palette.text)
            .line_break_mode(LineBreaking::WordWrap),
        label(body)
            .font("Liberation Sans")
            .text_size(14.0)
            .line_height(LineHeight::FontSizeRelative(1.6))
            .line_break_mode(LineBreaking::WordWrap)
            .color(state.appearance.palette.muted),
        sized_box(
            flex_row((cancel, affirmative))
                .main_axis_alignment(MainAxisAlignment::End)
                .gap(8.0.px()),
        )
        .padding(xilem::masonry::properties::Padding {
            top: 12.0.px(),
            ..Default::default()
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px());
    let observed = resize_observer(
        |state: &mut ShellState, size| {
            // ResizeObserver reports pixel-snapped bounds, while Portal tests
            // the unsnapped text extent. Keep one pixel for that rounding so a
            // naturally fitted dialog never acquires a fractional scroll range.
            let height = (size.height + 50.0).ceil() + 1.0;
            if height.is_finite() {
                state.frame_layout.confirm_height = Some(height);
            }
        },
        content,
    );
    let dialog =
        crate::ui::popup_group::group(title, portal(observed).constrain_horizontal(true)).dialog();
    crate::ui::search_focus::focus_on_activation(
        crate::ui::canvas_search::material::shell(
            sized_box(dialog)
                .padding(25.0.px())
                .width(width.px())
                .height(height.px()),
            state.appearance.palette,
        ),
        target,
        String::new(),
    )
    .with_dismiss(dismiss)
    .dismiss_directly()
}
