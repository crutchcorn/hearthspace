//! Retained Settings navigation: a sidebar on wide windows, an overlay on narrow ones.
use crate::{page_components::disclosure, state::SettingsApp, typography::label};
use hearthspace_ui_components::{
    self as components,
    icons::{Icon, icon},
};
use xilem::{
    AnyWidgetView, Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewId, ViewMarker, ViewPathTracker},
    masonry::{core::WidgetId, layout::AsUnit, properties::Padding},
    style::Style,
    view::{FlexSpacer, flex_row, sized_box},
};

mod surface;
mod widget;
pub(crate) use surface::NavigationMenu;
pub(crate) use widget::NavigationLayout as LayoutWidget;
use widget::{Close, NavigationLayout};

pub(super) const BREAKPOINT: f64 = 560.0;
pub(super) const HEADER_HEIGHT: f64 = 49.0;
pub(super) const MENU_HEIGHT: f64 = 282.0;

pub(crate) struct State {
    pub(crate) open: bool,
    pub(crate) width: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            open: false,
            width: 680.0,
        }
    }
}
impl State {
    pub(crate) fn compact(&self) -> bool {
        self.width < BREAKPOINT
    }
}

pub(super) fn view<P: WidgetView<SettingsApp>, N: WidgetView<SettingsApp>>(
    page: P,
    navigation: N,
    state: &SettingsApp,
) -> NavigationView<P, N> {
    NavigationView {
        page,
        navigation,
        open: state.navigation.open,
        compact: state.navigation.compact(),
    }
}

fn header(state: &SettingsApp, target: Option<WidgetId>) -> Box<AnyWidgetView<SettingsApp>> {
    let palette = state.palette();
    let control = disclosure(
        "Settings categories",
        state.navigation.open,
        false,
        flex_row((
            icon(Icon::PanelLeft, 16.0).color(palette.text),
            label(state.selected_category.label())
                .text_size(11.0)
                .color(palette.text),
        ))
        .gap(10.0.px()),
        |state: &mut SettingsApp| state.navigation.open = !state.navigation.open,
    );
    let control = if let Some(target) = target {
        control.controls(target)
    } else {
        control
    };
    Box::new(
        sized_box(
            flex_row((
                components::settings_button(
                    control
                        .height(40.0.px())
                        .border_width(1.0.px())
                        .padding(Padding::from_vh(9.0.px(), 10.0.px()))
                        .corner_radius(components::RADIUS_CONTROL.px()),
                    components::SettingsButton::Bare,
                    state.navigation.open,
                ),
                FlexSpacer::Flex(1.0),
            ))
            .gap(0.0.px()),
        )
        .padding(Padding {
            top: 4.0.px(),
            bottom: 5.0.px(),
            left: 8.0.px(),
            right: 8.0.px(),
        }),
    )
}

pub(super) struct NavigationView<P, N> {
    page: P,
    navigation: N,
    open: bool,
    compact: bool,
}
const PAGE: ViewId = ViewId::new(0);
const HEADER: ViewId = ViewId::new(1);
const NAVIGATION: ViewId = ViewId::new(2);

type Header = Box<AnyWidgetView<SettingsApp>>;
pub(super) struct ViewState<P: WidgetView<SettingsApp>, N: WidgetView<SettingsApp>> {
    page: P::ViewState,
    navigation: N::ViewState,
    header: <Header as View<SettingsApp, (), ViewCtx>>::ViewState,
    header_view: Header,
}
impl<P, N> ViewMarker for NavigationView<P, N> {}
impl<P: WidgetView<SettingsApp>, N: WidgetView<SettingsApp>> View<SettingsApp, (), ViewCtx>
    for NavigationView<P, N>
{
    type Element = Pod<NavigationLayout>;
    type ViewState = ViewState<P, N>;
    fn build(
        &self,
        ctx: &mut ViewCtx,
        state: &mut SettingsApp,
    ) -> (Self::Element, Self::ViewState) {
        let (page, page_state) = ctx.with_id(PAGE, |ctx| self.page.build(ctx, state));
        let (navigation, navigation_state) =
            ctx.with_id(NAVIGATION, |ctx| self.navigation.build(ctx, state));
        let menu = xilem::masonry::core::NewWidget::new(NavigationMenu::new(
            navigation.new_widget,
            self.compact,
        ));
        let header_view = header(state, Some(menu.id()));
        let (header, header_state) = ctx.with_id(HEADER, |ctx| header_view.build(ctx, state));
        let root = ctx.with_action_widget(|ctx| {
            ctx.create_pod(NavigationLayout::new(
                page.new_widget,
                header.new_widget,
                menu,
                self.compact,
                self.open,
            ))
        });
        (
            root,
            ViewState {
                page: page_state,
                navigation: navigation_state,
                header: header_state,
                header_view,
            },
        )
    }
    fn rebuild(
        &self,
        previous: &Self,
        states: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) {
        let header_view = header(state, Some(element.widget.navigation.id()));
        ctx.with_id(PAGE, |ctx| {
            self.page.rebuild(
                &previous.page,
                &mut states.page,
                ctx,
                NavigationLayout::page_mut(&mut element).downcast(),
                state,
            )
        });
        ctx.with_id(HEADER, |ctx| {
            header_view.rebuild(
                &states.header_view,
                &mut states.header,
                ctx,
                NavigationLayout::header_mut(&mut element).downcast(),
                state,
            )
        });
        ctx.with_id(NAVIGATION, |ctx| {
            NavigationLayout::with_navigation(&mut element, |mut child| {
                self.navigation.rebuild(
                    &previous.navigation,
                    &mut states.navigation,
                    ctx,
                    child.downcast(),
                    state,
                )
            })
        });
        states.header_view = header_view;
        NavigationLayout::configure(&mut element, self.compact, self.open);
    }
    fn teardown(
        &self,
        states: &mut Self::ViewState,
        ctx: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
    ) {
        ctx.with_id(PAGE, |ctx| {
            self.page.teardown(
                &mut states.page,
                ctx,
                NavigationLayout::page_mut(&mut element).downcast(),
            )
        });
        ctx.with_id(HEADER, |ctx| {
            states.header_view.teardown(
                &mut states.header,
                ctx,
                NavigationLayout::header_mut(&mut element).downcast(),
            )
        });
        ctx.with_id(NAVIGATION, |ctx| {
            NavigationLayout::with_navigation(&mut element, |mut child| {
                self.navigation
                    .teardown(&mut states.navigation, ctx, child.downcast())
            })
        });
        ctx.teardown_action_source(element);
    }
    fn message(
        &self,
        states: &mut Self::ViewState,
        message: &mut MessageCtx,
        mut element: Mut<'_, Self::Element>,
        state: &mut SettingsApp,
    ) -> MessageResult<()> {
        match message.take_first() {
            Some(PAGE) => self.page.message(
                &mut states.page,
                message,
                NavigationLayout::page_mut(&mut element).downcast(),
                state,
            ),
            Some(HEADER) => states.header_view.message(
                &mut states.header,
                message,
                NavigationLayout::header_mut(&mut element).downcast(),
                state,
            ),
            Some(NAVIGATION) => NavigationLayout::with_navigation(&mut element, |mut child| {
                self.navigation
                    .message(&mut states.navigation, message, child.downcast(), state)
            }),
            None if message.take_message::<Close>().is_some() => {
                state.navigation.open = false;
                MessageResult::Action(())
            }
            _ => MessageResult::Stale,
        }
    }
}
