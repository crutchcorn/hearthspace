use super::{ShellState, text};
use hearthspace_ui_components::{
    Palette,
    settings_material::{edge_light_with_palette, glass_edge_with_palette},
};
use xilem::{
    Pod, ViewCtx, WidgetView,
    core::{MessageCtx, MessageResult, Mut, View, ViewMarker},
    masonry::{
        accesskit::{Node, Role},
        core::{
            AccessCtx, BrushIndex, ChildrenIds, LayoutCtx, MeasureCtx, NoAction, PaintCtx,
            PropertiesMut, PropertiesRef, RegisterCtx, Update, UpdateCtx, Widget, render_text,
        },
        imaging::Painter,
        kurbo::{Affine, Axis, Size, Stroke},
        layout::{AsUnit, LenReq, Length},
        parley::Layout,
    },
};

pub(super) fn view(state: &ShellState) -> impl WidgetView<ShellState> + use<> {
    let prepared = &state.window_switcher.prepared;
    let snapshot = &prepared.snapshot;
    crate::ui::popup_group::group(
        format!(
            "{}, {} of {}",
            snapshot.title, snapshot.index, snapshot.count
        ),
        Caption {
            text: prepared.displayed.clone(),
            width: snapshot.caption.width,
            palette: state.appearance.palette,
        },
    )
    .status()
}

struct Caption {
    text: String,
    width: f64,
    palette: Palette,
}

impl ViewMarker for Caption {}
impl View<ShellState, (), ViewCtx> for Caption {
    type Element = Pod<CaptionWidget>;
    type ViewState = ();

    fn build(&self, ctx: &mut ViewCtx, _: &mut ShellState) -> (Self::Element, ()) {
        (
            ctx.create_pod(CaptionWidget {
                text: self.text.clone(),
                width: self.width,
                palette: self.palette,
                layout: Layout::default(),
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
        if self.text != previous.text || self.width != previous.width {
            element.widget.text.clone_from(&self.text);
            element.widget.width = self.width;
            element.ctx.request_layout();
        }
        if self.palette != previous.palette {
            element.widget.palette = self.palette;
            element.ctx.request_paint_only();
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

struct CaptionWidget {
    text: String,
    width: f64,
    palette: Palette,
    layout: Layout<BrushIndex>,
}

impl Widget for CaptionWidget {
    type Action = NoAction;
    fn register_children(&mut self, _: &mut RegisterCtx<'_>) {}
    fn update(&mut self, ctx: &mut UpdateCtx<'_>, _: &mut PropertiesMut<'_>, event: &Update) {
        if matches!(event, Update::FontsChanged) {
            ctx.request_layout();
        }
    }
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
            24.0.px()
        }
    }
    fn layout(&mut self, ctx: &mut LayoutCtx<'_>, _: &PropertiesRef<'_>, _: Size) {
        let (fonts, layouts) = ctx.text_contexts();
        text::shape(fonts, layouts, &mut self.layout, &self.text);
    }
    fn paint(&mut self, ctx: &mut PaintCtx<'_>, _: &PropertiesRef<'_>, painter: &mut Painter<'_>) {
        let rect = ctx.border_box();
        let shape = rect.inset(-0.5).to_rounded_rect(9.5);
        painter.fill(shape, self.palette.chrome_raised).draw();
        edge_light_with_palette(painter, rect, 10.0, &self.palette);
        glass_edge_with_palette(painter, rect, 10.0, false, &self.palette);
        painter
            .stroke(shape, &Stroke::new(1.0), self.palette.chrome_border)
            .draw();
        // Blur and outer shadow belong to the compositor so they extend past
        // the exact 24px native surface without clipping the CSS shadow.
        painter.with_fill_clip(shape, |painter| {
            render_text(
                painter,
                Affine::translate(((rect.width() - f64::from(self.layout.width())) / 2.0, 6.0)),
                &self.layout,
                &[self.palette.text.into()],
                true,
            );
        });
    }
    fn accessibility_role(&self) -> Role {
        Role::GenericContainer
    }
    fn accessibility(&mut self, _: &mut AccessCtx<'_>, _: &PropertiesRef<'_>, _: &mut Node) {}
    fn children_ids(&self) -> ChildrenIds {
        ChildrenIds::new()
    }
}
