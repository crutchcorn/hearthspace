//! Reference-sized power rows and confirmation, backed by real session actions.
use super::{Confirmation, power_confirmation, power_dialog, send_command};
use crate::ui::{
    canvas_search::material,
    popup_group,
    power_task::Action,
    search_focus::{FocusTarget, Record, focus_on_activation},
    state::{ShellOutputInstance, ShellState},
};
use hearthspace_ui_components::{
    self as components, Palette,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    dpi::LogicalSize,
    masonry::{
        layout::AsUnit,
        parley::style::LineHeight,
        properties::{LineBreaking, Padding},
    },
    style::Style,
    view::{
        CrossAxisAlignment, FlexExt, button, flex_col, flex_row, label, resize_observer, sized_box,
    },
};

#[derive(Debug, Default)]
pub(in crate::ui) struct PowerUiState {
    rows: [FocusTarget; 5],
    pub(super) cancel: FocusTarget,
    menu_height: Option<f64>,
    confirmation_height: Option<f64>,
}

pub(in crate::ui) fn size(state: &ShellState, output: &ShellOutputInstance) -> LogicalSize<f64> {
    let confirming = output.power_confirmation.is_some();
    let mut size = if confirming {
        LogicalSize::new(420.0, output.power_ui.confirmation_height.unwrap_or(162.0))
    } else {
        LogicalSize::new(224.0, output.power_ui.menu_height.unwrap_or(245.0))
    };
    size.height = size.height.min(400.0);
    if let Some(frame) = state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find(|frame| frame.output == output.key())
    {
        if frame.rect.width > 0.0 {
            let margin = if confirming { 48.0 } else { 24.0 };
            size.width = size.width.min((frame.rect.width - margin).max(1.0));
        }
        if frame.rect.height > 0.0 {
            size.height = size.height.min((frame.rect.height - 48.0).max(1.0));
        }
    }
    size
}

pub(super) fn observed<V: WidgetView<ShellState>>(
    output: &ShellOutputInstance,
    padding: f64,
    content: V,
) -> impl WidgetView<ShellState> + use<V> {
    let key = output.key().to_owned();
    let confirming = output.power_confirmation.is_some();
    resize_observer(
        move |state: &mut ShellState, size| {
            if !size.height.is_finite() {
                return;
            }
            if let Some(output) = state.outputs.iter_mut().find(|output| output.key() == key) {
                // The menu's group already rounds its intrinsic height. The
                // dialog portal needs one extra pixel for snapped text metrics.
                let height =
                    Some(size.height.ceil() + padding * 2.0 + if confirming { 1.0 } else { 0.0 });
                if confirming {
                    output.power_ui.confirmation_height = height;
                } else {
                    output.power_ui.menu_height = height;
                }
            }
        },
        content,
    )
}

fn row<F: Fn(&mut ShellState) + Send + Sync + 'static>(
    glyph: Icon,
    name: &'static str,
    enabled: bool,
    color: xilem::Color,
    target: FocusTarget,
    callback: F,
) -> impl WidgetView<ShellState> + use<F> {
    Record {
        inner: components::button_surface(
            button(
                flex_row((
                    icon(glyph, 18.0).color(color),
                    label(name)
                        .font("Liberation Sans")
                        .text_size(13.0)
                        .line_height(LineHeight::FontSizeRelative(1.4))
                        .color(color)
                        .flex(1.0),
                ))
                .gap(12.0.px()),
                callback,
            )
            .disabled(!enabled)
            .height(44.0.px())
            // The one-pixel native focus border is included in the 12px inset.
            .padding(Padding::from_vh(0.0.px(), 11.0.px()))
            .corner_radius(10.0.px()),
            components::ButtonSurface::Bare,
            false,
        ),
        target,
    }
}

pub(super) fn feedback(message: String, palette: Palette) -> impl WidgetView<ShellState> + use<> {
    popup_group::group(
        "Power status",
        sized_box(
            label(message)
                .font("Liberation Sans")
                .text_size(13.0)
                .line_height(LineHeight::FontSizeRelative(1.4))
                .line_break_mode(LineBreaking::WordWrap)
                .color(palette.text_muted),
        )
        .padding(Padding::from_vh(8.0.px(), 12.0.px())),
    )
    .status()
}

fn menu(state: &ShellState, output: &ShellOutputInstance) -> impl WidgetView<ShellState> + use<> {
    let palette = state.appearance.palette.for_glass();
    let power = &state.power;
    let targets = &output.power_ui.rows;
    let lock_key = output.key().to_owned();
    let suspend_key = lock_key.clone();
    let unavailable = (!power.status.available && !power.busy()).then(|| {
        feedback(
            power
                .status
                .error
                .clone()
                .unwrap_or_else(|| "Suspend is unavailable in this session.".into()),
            palette,
        )
    });
    let mut navigation = vec![targets[0].clone()];
    if power.can_suspend() {
        navigation.push(targets[1].clone());
    }
    let confirmations: Vec<_> = Confirmation::ALL
        .into_iter()
        .enumerate()
        .map(|(index, action)| {
            let enabled = action.enabled(state);
            let key = output.key().to_owned();
            let target = targets[index + 2].clone();
            if enabled {
                navigation.push(target.clone());
            }
            row(
                match action {
                    Confirmation::LogOff => Icon::LogOut,
                    Confirmation::Reboot => Icon::RotateCcw,
                    Confirmation::PowerOff => Icon::Power,
                },
                action.label(),
                enabled,
                if enabled {
                    palette.text
                } else {
                    palette.text_muted
                },
                target,
                move |state| power_confirmation::open(state, &key, action),
            )
        })
        .collect();
    let capability_errors: Vec<_> = [
        (!power.status.reboot_available).then_some(&power.status.reboot_error),
        (!power.status.power_off_available).then_some(&power.status.power_off_error),
    ]
    .into_iter()
    .flatten()
    .filter_map(|error| error.as_ref())
    .map(|error| feedback(error.clone(), palette))
    .collect();
    let rows = flex_col((
        output
            .launch_error
            .clone()
            .map(|error| feedback(error, palette)),
        unavailable,
        capability_errors,
        row(
            Icon::Lock,
            "Lock screen",
            true,
            palette.accent_strong,
            targets[0].clone(),
            move |state| {
                let result = send_command(&state.command_socket, "lock-session");
                state.finish_launch(&lock_key, result);
            },
        ),
        row(
            Icon::Moon,
            if power.pending_action == Some(Action::Suspend) {
                "Suspending…"
            } else {
                "Suspend"
            },
            power.can_suspend(),
            if power.can_suspend() {
                palette.text
            } else {
                palette.text_muted
            },
            targets[1].clone(),
            move |state| crate::ui::power_task::suspend(state, &suspend_key),
        ),
        sized_box(
            sized_box(flex_col(()))
                .height(1.0.px())
                .background_color(palette.border),
        )
        .padding(Padding::from_vh(5.0.px(), 12.0.px())),
        confirmations,
    ))
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .gap(0.0.px());
    // Events bubble outward: menu navigation must run before the portal's
    // default Arrow/Home/End scrolling, including on a short output.
    let rows = focus_on_activation(
        popup_group::group("Power", observed(output, 7.0, rows)).menu(),
        targets[0].clone(),
        output.key().to_owned(),
    )
    .with_arrow_navigation(navigation)
    .with_dismiss(ShellState::dismiss_power)
    .with_focus_recovery()
    .dismiss_directly();
    let content = components::touch_portal(rows)
        .constrain_horizontal(true)
        .must_fill(false);
    material::panel(sized_box(content).padding(7.0.px()), false, palette)
}

pub(in crate::ui) fn panel(
    state: &ShellState,
    output: &ShellOutputInstance,
) -> Box<AnyWidgetView<ShellState>> {
    if let Some(action) = output.power_confirmation {
        power_dialog::view(state, output, action).boxed()
    } else {
        menu(state, output).boxed()
    }
}
