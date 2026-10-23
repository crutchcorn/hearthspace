//! Decode app-aware artwork on a worker and expose shared application glyphs.
use std::collections::BTreeMap;

use hearthspace_ui_components as components;
use xilem::{
    Pod, ViewCtx,
    core::{MessageCtx, MessageProxy, MessageResult, Mut, NoElement, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx, PropertiesRef,
            RegisterCtx, Widget, WidgetId,
        },
        imaging::Painter,
        kurbo::{Affine, Axis, Circle, Line, Rect, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
        peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    },
    view::task_raw,
};

use super::{ShellState, apps};
use crate::{
    app_catalog::DesktopApp,
    tray::{TrayIcon, TrayPixmap},
};

const MAX_ICONS: usize = 128;
type Icons = BTreeMap<CacheKey, Option<TrayPixmap>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum CacheKey {
    App {
        id: String,
        argv: Option<Vec<String>>,
        icon: Option<String>,
    },
    Named(String),
}

impl CacheKey {
    pub(super) fn app(app: &DesktopApp) -> Self {
        Self::App {
            id: app.id.clone(),
            argv: app.launch_argv().ok(),
            icon: app.icon.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Request {
    App(Box<DesktopApp>),
    Named(String),
}

impl Request {
    pub(super) fn app(app: &DesktopApp) -> Self {
        Self::App(Box::new(app.clone()))
    }

    pub(super) fn key(&self) -> CacheKey {
        match self {
            Self::App(app) => CacheKey::app(app),
            Self::Named(name) => CacheKey::Named(name.clone()),
        }
    }
}

pub(super) fn requests(state: &ShellState) -> Vec<Request> {
    let dock = apps(state).into_iter().filter_map(|app| app.icon);
    let catalog = state
        .catalog
        .apps()
        .iter()
        .filter(|app| app.icon.is_some())
        .map(Request::app);
    // Visible pins keep priority even in a large catalog. Distinct applications
    // must not share a cache entry just because both declare Icon=steam.
    let mut requests = BTreeMap::new();
    for request in dock.chain(catalog) {
        requests.insert(request.key(), request);
        if requests.len() == MAX_ICONS {
            break;
        }
    }
    requests.into_values().collect()
}

fn decode_requests(mut cache: Icons, requests: Vec<Request>) -> Icons {
    let requests: BTreeMap<_, _> = requests.into_iter().map(|r| (r.key(), r)).collect();
    cache.retain(|key, _| requests.contains_key(key));
    for (key, request) in requests {
        cache.entry(key).or_insert_with(|| match request {
            Request::App(app) => crate::app_icon::resolve_app(&app),
            Request::Named(name) => crate::app_icon::resolve(&TrayIcon {
                name: Some(name),
                ..Default::default()
            }),
        });
    }
    cache
}

pub(in crate::ui) fn task() -> impl View<ShellState, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy: MessageProxy<Icons>, state: &mut ShellState| {
            let (sender, mut receiver) = xilem::tokio::sync::watch::channel(requests(state));
            state.dock.requests = Some(sender);
            async move {
                let mut cache = Icons::new();
                loop {
                    let requests = receiver.borrow_and_update().clone();
                    let result = xilem::tokio::task::spawn_blocking(move || {
                        decode_requests(cache, requests)
                    })
                    .await;
                    let Ok(icons) = result else { break };
                    cache = icons;
                    if proxy.message(cache.clone()).is_err() || receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        },
        |state: &mut ShellState, icons| {
            state.dock.images = icons
                .into_iter()
                .map(|(name, pixmap)| (name, pixmap.map(image)))
                .collect();
        },
    )
}

fn image(pixmap: TrayPixmap) -> ImageBrush {
    ImageData {
        data: pixmap.rgba.into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: pixmap.width,
        height: pixmap.height,
    }
    .into()
}

#[cfg(test)]
#[path = "dock_icon_tests.rs"]
mod tests;

#[derive(Clone, PartialEq)]
pub(super) enum Glyph {
    App {
        image: Option<ImageBrush>,
        count: usize,
        focused: bool,
    },
    Next,
}

pub(super) struct IconView {
    pub glyph: Glyph,
    pub name: String,
    pub width: f64,
}

impl ViewMarker for IconView {}
impl View<ShellState, (), ViewCtx> for IconView {
    type Element = Pod<IconWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.create_pod(IconWidget {
                glyph: self.glyph.clone(),
                name: self.name.clone(),
                width: self.width,
            }),
            (),
        )
    }

    fn rebuild(
        &self,
        previous: &Self,
        _: &mut (),
        _: &mut ViewCtx,
        mut element: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) {
        if self.glyph != previous.glyph {
            element.widget.glyph = self.glyph.clone();
            element.ctx.request_render();
        }
        if self.width != previous.width {
            element.widget.width = self.width;
            element.ctx.request_layout();
        }
        if self.name != previous.name {
            element.widget.name = self.name.clone();
            element.ctx.request_accessibility_update();
        }
    }

    fn teardown(&self, _: &mut (), _: &mut ViewCtx, _: Mut<'_, Self::Element>) {}

    fn message(
        &self,
        _: &mut (),
        _: &mut MessageCtx,
        _: Mut<'_, Self::Element>,
        _: &mut ShellState,
    ) -> MessageResult<()> {
        MessageResult::Stale
    }
}

pub(super) struct IconWidget {
    glyph: Glyph,
    name: String,
    width: f64,
}

impl Widget for IconWidget {
    type Action = NoAction;
    fn property_changed(
        &mut self,
        ctx: &mut xilem::masonry::core::UpdateCtx<'_>,
        property: std::any::TypeId,
    ) {
        components::Palette::prop_changed(ctx, property);
    }

    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}

    fn measure(
        &mut self,
        _: &mut MeasureCtx<'_>,
        _: &PropertiesRef<'_>,
        axis: Axis,
        _: LenReq,
        _: Option<Length>,
    ) -> Length {
        if axis == Axis::Horizontal {
            self.width.px()
        } else {
            36.0.px()
        }
    }

    fn layout(&mut self, _: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {}

    fn paint(
        &mut self,
        ctx: &mut PaintCtx<'_>,
        props: &PropertiesRef<'_>,
        painter: &mut Painter<'_>,
    ) {
        let palette = *props.get::<components::Palette>(ctx.property_cache());
        self.glyph.paint(self.width, &palette, painter);
    }

    fn accessibility_role(&self) -> Role {
        Role::Image
    }

    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, node: &mut Node) {
        node.set_label(self.name.clone());
    }

    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }

    fn make_trace_span(&self, id: WidgetId) -> tracing::Span {
        tracing::trace_span!("DockIcon", id = id.trace())
    }
}

impl Glyph {
    fn paint(&self, width: f64, palette: &components::Palette, painter: &mut Painter<'_>) {
        let stroke = Stroke::new(1.6);
        let center = width / 2.0;
        match self {
            Glyph::App {
                image,
                count,
                focused,
            } => {
                if let Some(image) = image {
                    let scale = 24.0 / f64::from(image.image.width.max(image.image.height).max(1));
                    painter.draw_image(
                        image,
                        Affine::translate((
                            (width - f64::from(image.image.width) * scale) / 2.0,
                            (30.0 - f64::from(image.image.height) * scale) / 2.0,
                        )) * Affine::scale(scale),
                    );
                } else {
                    // A recognizable app window replaces missing theme assets.
                    painter
                        .stroke(
                            Rect::new(center - 10.0, 6.0, center + 10.0, 24.0),
                            &stroke,
                            palette.text,
                        )
                        .draw();
                    painter
                        .stroke(
                            Line::new((center - 10.0, 11.0), (center + 10.0, 11.0)),
                            &stroke,
                            palette.text,
                        )
                        .draw();
                    painter
                        .fill(Circle::new((center - 7.0, 8.5), 0.8), palette.text)
                        .draw();
                }
                let color = if *focused {
                    palette.accent
                } else {
                    palette.text_muted
                };
                let dots = (*count).min(3);
                for index in 0..dots {
                    let x = center + (index as f64 - (dots - 1) as f64 / 2.0) * 6.0;
                    painter.fill(Circle::new((x, 32.0), 1.5), color).draw();
                }
            }
            Glyph::Next => {
                if width <= 2.0 {
                    return;
                }
                let centers = if width < 26.0 {
                    vec![width / 2.0]
                } else {
                    vec![center - 3.5, center + 3.5]
                };
                let span = ((width - 2.0) / 2.0).min(3.0);
                for x in centers {
                    painter
                        .stroke(
                            Line::new((x - span, 12.0), (x + span, 18.0)),
                            &stroke,
                            palette.text,
                        )
                        .draw();
                    painter
                        .stroke(
                            Line::new((x + span, 18.0), (x - span, 24.0)),
                            &stroke,
                            palette.text,
                        )
                        .draw();
                }
            }
        }
    }
}
