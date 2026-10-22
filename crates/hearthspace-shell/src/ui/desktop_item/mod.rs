//! Native, transparent desktop tiles matching the experiment's canvas items.
use hearthspace_ui_components::Palette;
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{core::DefaultProperties, peniko::ImageBrush},
};

mod label;
mod paint;
mod surface;
mod widget;
use surface::TileSurface;
pub(super) use widget::DesktopTile;
#[cfg(test)]
mod tests;

pub(super) const MARGIN: f64 = hearthspace_common::DESKTOP_ITEM_MARGIN as f64;
pub(super) const WIDTH: f64 = hearthspace_common::DESKTOP_SURFACE_WIDTH as f64;
pub(super) const HEIGHT: f64 = hearthspace_common::DESKTOP_SURFACE_HEIGHT as f64;
const ITEM_WIDTH: f64 = hearthspace_common::DESKTOP_ITEM_WIDTH as f64;
const ITEM_HEIGHT: f64 = hearthspace_common::DESKTOP_ITEM_HEIGHT as f64;

#[derive(Clone, PartialEq)]
pub(super) enum Artwork {
    App(Option<ImageBrush>),
    Settings,
    Folder,
    File(Option<ImageBrush>),
}

#[derive(Clone, PartialEq)]
pub(super) struct TileContent {
    pub name: String,
    pub artwork: Artwork,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct TileState {
    pub selected: bool,
    pub focused: bool,
    pub keyboard_focus: bool,
    pub hovered: bool,
    pub photo_background: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileAction {
    Activate,
    Focus,
}

pub(super) fn view<F>(content: TileContent, state: TileState, action: F) -> TileView<F> {
    TileView {
        content,
        state,
        action,
    }
}

pub(super) struct TileView<F> {
    content: TileContent,
    state: TileState,
    action: F,
}

impl<F> ViewMarker for TileView<F> {}
impl<State: 'static, F> View<State, (), ViewCtx> for TileView<F>
where
    F: Fn(&mut State, TileAction) + Send + Sync + 'static,
{
    type Element = Pod<TileSurface>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut State) -> (Self::Element, ()) {
        let child = ctx.with_action_widget(|ctx| {
            ctx.create_pod(DesktopTile::new(self.content.clone(), self.state))
        });
        (ctx.create_pod(TileSurface::new(child.new_widget)), ())
    }

    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut State,
    ) {
        if self.content != previous.content || self.state != previous.state {
            DesktopTile::set(
                &mut TileSurface::child_mut(&mut element),
                self.content.clone(),
                self.state,
            );
        }
    }

    fn teardown(&self, _: &mut (), ctx: &mut ViewCtx, mut element: Mut<'_, Self::Element>) {
        ctx.teardown_action_source(TileSurface::child_mut(&mut element));
    }

    fn message(
        &self,
        _: &mut (),
        message: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        state: &mut State,
    ) -> MessageResult<()> {
        if let Some(action) = message.take_message::<TileAction>() {
            (self.action)(state, *action);
            MessageResult::Action(())
        } else {
            MessageResult::Stale
        }
    }
}

pub(super) fn theme_properties(properties: &mut DefaultProperties, palette: Palette) {
    // Desktop ink uses the reference's exact colors rather than glass contrast ink.
    properties.insert::<DesktopTile, _>(Palette::new(palette.appearance).with_photo(palette.photo));
}
