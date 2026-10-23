//! Frame shortcuts keep the desktop identity, app artwork, and launch path.
use crate::ui::{
    desktop_item::{Artwork, TileContent},
    state::ShellState,
};
use hearthspace_common::frame_layout::{LayoutTarget, TargetKind};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, WidgetView,
    masonry::layout::AsUnit,
    style::Style,
    view::{button, image, sized_box},
};

pub(in crate::ui) fn presentation(state: &ShellState, id: &str) -> Option<(String, bool)> {
    let target = LayoutTarget::new(TargetKind::Item, id);
    state
        .frame_layout
        .snapshot
        .outputs
        .iter()
        .find_map(|output| {
            output.layout.placement(&target).is_frame().then(|| {
                (
                    output.output.clone(),
                    output.layout.group_for(&target).is_some(),
                )
            })
        })
}

pub(in crate::ui) fn view(
    state: &ShellState,
    content: TileContent,
    id: String,
    output: String,
    grouped: bool,
) -> Box<AnyWidgetView<ShellState>> {
    let target = LayoutTarget::new(TargetKind::Item, &id);
    let chain = state
        .frame_layout
        .chains
        .get(&target.key())
        .cloned()
        .unwrap_or_default();
    let request = state
        .frame_layout
        .snapshot
        .focus_request
        .clone()
        .filter(|request| request.target == target);
    let artwork: Box<AnyWidgetView<ShellState>> = match content.artwork {
        Artwork::App(Some(brush)) | Artwork::File(Some(brush)) => {
            image(brush).alt_text(content.name.clone()).boxed()
        }
        Artwork::App(None) => icon(Icon::AppsGrid, 30.0).label(&content.name).boxed(),
        Artwork::Settings => icon(Icon::AppSettings, 24.0).label(&content.name).boxed(),
        Artwork::Folder => icon(Icon::Folder, 30.0).label(&content.name).boxed(),
        Artwork::File(None) => icon(Icon::FileText, 28.0).label(&content.name).boxed(),
    };
    let size = if grouped { 44.0 } else { 54.0 };
    let control = components::button_surface(
        button(
            sized_box(artwork).width(32.0.px()).height(32.0.px()),
            move |state: &mut ShellState| crate::ui::desktop::activate(state, &id),
        )
        .padding(if grouped { 5.0 } else { 10.0 }.px())
        .width(size.px())
        .height(size.px())
        .corner_radius(if grouped { 15.0 } else { 20.0 }.px()),
        components::ButtonSurface::Bare,
        false,
    );
    let content = super::focus::item(control, &chain, 0);
    super::focus::boundary(
        super::material(content, !grouped, state),
        chain,
        output,
        target,
        request,
    )
    .boxed()
}
