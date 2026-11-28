//! Output-centered system confirmation; the compositor owns scrim and timeout.
use crate::{state::SettingsApp, typography::label};
use hearthspace_ipc::display_modes::Request;
use hearthspace_ui_components::{
    self as components,
    controls::{self, ButtonVariant},
};
use xilem::{
    WidgetView, WindowId, WindowOptionsExtLinux, WindowView,
    dpi::LogicalSize,
    masonry::{
        core::DefaultProperties,
        layout::AsUnit,
        parley::style::{FontWeight, LineHeight},
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{CrossAxisAlignment, MainAxisAlignment, flex_col, flex_row, resize_observer, sized_box},
};

const TITLE: &str = "Keep these display settings?";

pub(crate) fn window(
    state: &mut SettingsApp,
    properties: std::sync::Arc<DefaultProperties>,
) -> Option<WindowView<SettingsApp>> {
    let modes = &mut state.display_layout.modes;
    let Some(preview) = modes.snapshot.preview.clone() else {
        modes.dialog = None;
        modes.dismissed_token = None;
        modes.dialog_height = None;
        return None;
    };
    if modes.dismissed_token == Some(preview.token) {
        return None;
    }
    let id = match modes.dialog {
        Some((token, id)) if token == preview.token => id,
        _ => {
            let id = WindowId::next();
            modes.dialog = Some((preview.token, id));
            id
        }
    };
    let size = LogicalSize::new(420.0, modes.dialog_height.unwrap_or(230.0));
    let app_id = hearthspace_common::display_mode_confirm_app_id_for_output(&preview.output);
    Some(
        xilem::window(id, TITLE, view(state))
            .with_base_color(xilem::Color::TRANSPARENT)
            .with_default_properties(properties)
            .with_options(move |options| {
                options
                    .with_name(&app_id, &app_id)
                    .with_transparent(true)
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_initial_inner_size(size)
                    .with_min_inner_size(size)
                    .with_max_inner_size(size)
                    .on_close(move |state: &mut SettingsApp| {
                        state.display_layout.modes.dismissed_token = Some(preview.token);
                        crate::display_modes::request(
                            state,
                            Request::Revert {
                                token: preview.token,
                            },
                        );
                    })
            }),
    )
}

pub(crate) fn view(state: &SettingsApp) -> impl WidgetView<SettingsApp> + use<> {
    let modes = &state.display_layout.modes;
    let preview = modes
        .snapshot
        .preview
        .as_ref()
        .expect("active mode confirmation");
    let token = preview.token;
    let palette = state.palette();
    let body = if preview.ready {
        format!(
            "Reverting in {} seconds unless you keep these settings.",
            preview.seconds_remaining
        )
    } else {
        "Waiting for the new mode to be presented. Changes revert automatically if it fails.".into()
    };
    let content = flex_col((
        label(TITLE)
            .text_size(19.0)
            .weight(FontWeight::BOLD)
            .line_height(LineHeight::FontSizeRelative(1.0))
            .color(palette.text)
            .line_break_mode(LineBreaking::WordWrap),
        label(body)
            .text_size(14.0)
            .line_height(LineHeight::FontSizeRelative(1.6))
            .color(palette.muted)
            .line_break_mode(LineBreaking::WordWrap),
        modes
            .error
            .clone()
            .map(|e| crate::page_components::note(palette, e)),
        sized_box(
            flex_row((
                crate::page_components::focus_on_mount(controls::form_button(
                    palette,
                    "Revert",
                    ButtonVariant::Neutral,
                    modes.busy,
                    move |s| crate::display_modes::request(s, Request::Revert { token }),
                )),
                controls::form_button(
                    palette,
                    "Keep changes",
                    ButtonVariant::Primary,
                    modes.busy || !preview.ready,
                    move |s| crate::display_modes::request(s, Request::Keep { token }),
                ),
            ))
            .main_axis_alignment(MainAxisAlignment::End)
            .gap(8.0.px()),
        )
        .padding(Padding {
            top: 12.0.px(),
            left: controls::style::FOCUS_RING_OUTSET.px(),
            right: controls::style::FOCUS_RING_OUTSET.px(),
            bottom: controls::style::FOCUS_RING_OUTSET.px(),
        }),
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(12.0.px());
    let measured = resize_observer(
        |state: &mut SettingsApp, size| {
            if size.height.is_finite() {
                state.display_layout.modes.dialog_height = Some(size.height.ceil() + 51.0);
            }
        },
        content,
    );
    components::settings_material::dialog_surface(
        sized_box(crate::page_components::dialog(
            components::touch_portal(measured)
                .constrain_horizontal(true)
                .must_fill(false),
            TITLE,
        ))
        .padding(25.0.px()),
    )
}
