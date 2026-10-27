use smithay::{
    backend::renderer::{
        Color32F, RendererSuper,
        damage::OutputDamageTracker,
        element::{
            Kind,
            memory::MemoryRenderBufferRenderElement,
            render_elements,
            solid::SolidColorRenderElement,
            surface::{WaylandSurfaceRenderElement, render_elements_from_surface_tree},
            texture::TextureRenderElement,
            utils::{CropRenderElement, Relocate, RelocateRenderElement, RescaleRenderElement},
        },
        gles::{GlesRenderer, element::PixelShaderElement},
    },
    desktop::PopupManager,
    input::pointer::CursorImageAttributes,
    utils::{IsAlive, Logical, Physical, Point, Rectangle, Size},
    wayland::{
        compositor::{SurfaceAttributes, TraversalAction, with_states, with_surface_tree_downward},
        shell::xdg::SurfaceCachedState,
    },
};
use std::sync::Mutex;
use wayland_server::protocol::wl_surface;

use super::{App, ManagedWindowKind, window_frame};

mod background;
mod blur;
mod blur_shader;
mod layout_clip;
mod layout_glass;
mod layout_overlays;
mod lock;
mod order;
mod titlebar;
mod window_backdrop;
mod window_backdrop_shader;

pub(in crate::compositor) use background::BackgroundImageTile;
pub(in crate::compositor) use blur::CornerBlurCache;
pub(in crate::compositor) use window_backdrop::WindowBackdropCache;

type GlesTexture = <GlesRenderer as RendererSuper>::TextureId;

render_elements! {
    pub(super) HearthspaceRenderElement<=GlesRenderer>;
    Surface = RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Memory = MemoryRenderBufferRenderElement<GlesRenderer>,
    Frame = PixelShaderElement,
    RoundedMemory = window_frame::RoundedElement<MemoryRenderBufferRenderElement<GlesRenderer>>,
    RoundedSurface = window_frame::RoundedElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>,
    Texture = TextureRenderElement<GlesTexture>,
    ClippedSurface = CropRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>,
    Solid = SolidColorRenderElement,
    Turn = super::layout_editor::TurnElement,
    CroppedFrame = CropRenderElement<PixelShaderElement>,
    CroppedMemory = CropRenderElement<MemoryRenderBufferRenderElement<GlesRenderer>>,
    CroppedRoundedMemory = CropRenderElement<window_frame::RoundedElement<MemoryRenderBufferRenderElement<GlesRenderer>>>,
    CroppedRoundedSurface = CropRenderElement<window_frame::RoundedElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>>,
    CroppedTexture = CropRenderElement<TextureRenderElement<GlesTexture>>,
}

render_elements! {
    pub(super) TranslatedHearthspaceRenderElement<=GlesRenderer>;
    Scaled = RescaleRenderElement<RelocateRenderElement<HearthspaceRenderElement>>,
    Unscaled = RelocateRenderElement<HearthspaceRenderElement>,
}

/// Result of rendering one frame: the damaged output regions to submit, or
/// `None` when nothing changed and the frame can be skipped.
type RenderFrameResult = Result<Option<Vec<Rectangle<i32, Physical>>>, Box<dyn std::error::Error>>;

struct RenderScene {
    elements: Vec<HearthspaceRenderElement>,
    /// Everything below the shell, switcher, and cursor may feed corner blur.
    desktop_start: usize,
    window_backdrops: Vec<window_backdrop::BackdropRequest>,
    window_spans: Vec<(u64, std::ops::Range<usize>)>,
}

impl RenderScene {
    fn overlay(elements: Vec<HearthspaceRenderElement>) -> Self {
        Self {
            desktop_start: elements.len(),
            elements,
            window_backdrops: Vec::new(),
            window_spans: Vec::new(),
        }
    }
}

impl App {
    /// Render the current scene through an [`OutputDamageTracker`].
    ///
    /// The damage tracker compares this frame's elements against the previous
    /// frame and only clears and redraws the regions that actually changed,
    /// rather than repainting the whole output every frame. The returned value
    /// is the damaged region (in output coordinates) to hand to the backend's
    /// buffer swap, or `None` when nothing changed and the frame can be skipped.
    ///
    /// This is backend-agnostic: it only depends on a [`GlesRenderer`] and the
    /// shared [`App`] state, so both the winit and (future) DRM backends share
    /// it. Backend-specific buffer binding, frame submission, and client frame
    /// callbacks live in the backend's own render path.
    pub(super) fn render_frame(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        damage_tracker: &mut OutputDamageTracker,
        age: usize,
    ) -> RenderFrameResult {
        self.render_frame_at(
            renderer,
            framebuffer,
            damage_tracker,
            age,
            Rectangle::from_size(self.output_logical_size()),
            self.primary_output_scale(),
        )
    }

    pub(super) fn render_frame_at(
        &mut self,
        renderer: &mut GlesRenderer,
        framebuffer: &mut <GlesRenderer as RendererSuper>::Framebuffer<'_>,
        damage_tracker: &mut OutputDamageTracker,
        age: usize,
        output_rect: Rectangle<i32, Logical>,
        output_scale: i32,
    ) -> RenderFrameResult {
        let output_location = output_rect.loc;
        let output_offset = Point::<i32, Physical>::from((-output_location.x, -output_location.y));
        let output_scale = output_scale.max(1);
        let render_scale = f64::from(output_scale);
        let scene = self.collect_render_elements(renderer, output_rect);
        let mut elements = scene
            .elements
            .into_iter()
            .map(|element| {
                let relocated =
                    RelocateRenderElement::from_element(element, output_offset, Relocate::Relative);
                TranslatedHearthspaceRenderElement::from(RescaleRenderElement::from_element(
                    relocated,
                    Point::<i32, Physical>::from((0, 0)),
                    render_scale,
                ))
            })
            .collect::<Vec<_>>();

        // Background tiles already carry their physical presentation size.
        // Custom wallpapers preserve pixel sizing; the default grid accounts
        // for monitor DPI before this output-local translation.
        let tiles = if self.session_locked()
            || (!self.canvas_preview_presenting() && self.fullscreen_on_output(output_rect))
        {
            Vec::new()
        } else {
            self.background_image_elements(renderer, output_rect, output_scale)
        };
        for tile in tiles {
            elements.push(TranslatedHearthspaceRenderElement::from(
                RelocateRenderElement::from_element(
                    HearthspaceRenderElement::from(tile),
                    output_offset,
                    Relocate::Relative,
                ),
            ));
        }
        let remap = self.apply_window_backdrops(
            renderer,
            output_rect,
            output_scale,
            &scene.window_backdrops,
            &mut elements,
        );
        let desktop_start = remap[scene.desktop_start];
        let mut spans: Vec<_> = scene
            .window_spans
            .into_iter()
            .map(|(id, range)| (id, remap[range.start]..remap[range.end]))
            .collect();
        let blurred = self.corner_blur_elements(
            renderer,
            output_rect,
            output_scale,
            &elements[desktop_start..],
        );
        let blur_count = blurred.len();
        elements.splice(
            desktop_start..desktop_start,
            blurred.into_iter().map(|element| {
                TranslatedHearthspaceRenderElement::from(RelocateRenderElement::from_element(
                    HearthspaceRenderElement::from(element),
                    (0, 0),
                    Relocate::Relative,
                ))
            }),
        );
        for (_, range) in &mut spans {
            if range.start >= desktop_start {
                range.start += blur_count;
            }
            if range.end > desktop_start {
                range.end += blur_count;
            }
        }
        self.apply_layout_turns(renderer, &mut elements, &spans, output_rect);
        let result = damage_tracker.render_output(
            renderer,
            framebuffer,
            age,
            &elements,
            // The repeated background image is opaque and covers the output.
            Color32F::new(0.0, 0.0, 0.0, 1.0),
        )?;
        Ok(result.damage.cloned())
    }

    /// Collect every render element for the current frame in front-to-back
    /// (topmost-first) order, as expected by [`OutputDamageTracker`].
    ///
    /// Window surfaces are built at native scale and wrapped in a
    /// [`RescaleRenderElement`] so the viewport zoom is applied uniformly while
    /// the damage tracker still works in a single output coordinate space.
    fn collect_render_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output_rect: Rectangle<i32, Logical>,
    ) -> RenderScene {
        self.window_frames.set_appearance(self.appearance);
        if self.session_locked() {
            self.window_frames.clear();
            return RenderScene::overlay(self.collect_lock_elements(renderer, output_rect));
        }
        self.refresh_normal_window_outputs();
        self.refresh_fractional_scales();
        self.window_frames.retain(
            &self
                .windows
                .iter()
                .enumerate()
                .filter(|(index, window)| {
                    window.mapped
                        && (window.kind == ManagedWindowKind::Normal
                            || self.is_canvas_map_panel(*index)
                            || self.canvas_search_panel_allowed(*index)
                            || self.canvas_search_result_visible(*index)
                            || self.window_switcher_caption_visible(*index)
                            || self.layout_glass_rect(*index).is_some())
                })
                .map(|(_, window)| window.id)
                .collect(),
        );

        let mut elements = Vec::new();
        for element in self.software_cursor_elements(renderer, output_rect) {
            elements.push(element);
        }

        let mut group_elements = self.collect_group_elements(renderer, output_rect);
        let mut layout = self.layout_overlay_elements(renderer, output_rect);
        let mut desktop_start = None;
        let mut desktop_selection = self.desktop_selection_elements(renderer);
        let mut window_backdrops = Vec::new();
        let hovered_window = self.hovered_normal_window_id();
        let mut ordered = order::render_order(self.windows.iter().map(|window| window.kind));
        ordered.sort_by_key(|index| {
            std::cmp::Reverse((
                self.is_power_confirmation(*index),
                self.layout_render_layer(*index).unwrap_or_else(|| {
                    match self.windows[*index].kind {
                        ManagedWindowKind::ControlOsd => 130,
                        ManagedWindowKind::Launcher => 110,
                        ManagedWindowKind::ShellBar | ManagedWindowKind::ShellWatcher => 100,
                        ManagedWindowKind::Normal => 10,
                        ManagedWindowKind::DesktopItem => 0,
                    }
                }),
            ))
        });
        let mut window_spans = Vec::new();
        for index in ordered {
            let layer = self.layout_render_layer(index).unwrap_or_else(|| {
                if self.windows[index].kind.is_shell_chrome() {
                    110
                } else {
                    10
                }
            });
            // Editing guides and highlights sit below frame controls (100)
            // and their shared glass (99), so they cannot draw across a tool.
            if layer < 99 {
                append_layout_outlines(
                    &mut elements,
                    &mut layout.front,
                    &mut layout.owners,
                    &mut window_spans,
                );
            }
            if layer < 100 {
                desktop_start.get_or_insert(elements.len());
            }
            if self.windows[index].kind == ManagedWindowKind::DesktopItem {
                elements.append(&mut desktop_selection);
            }
            if self.windows[index].kind == ManagedWindowKind::Normal {
                elements.append(&mut group_elements.overlay);
            }
            if !self.window_visible_on_output(index, output_rect) {
                continue;
            }
            let front = elements.len();
            // Each icon/control keeps its own selection ring. Higher objects
            // cover it, while a held object carries its ring above the guides.
            if let Some(outlines) = layout.objects.remove(&self.windows[index].id) {
                elements.extend(outlines);
            }
            let projection = self.window_projection(index);
            elements.extend(self.canvas_preview_selection_elements(renderer, index, projection));
            // Popups (e.g. menus) render above their window's content and chrome.
            for popup in self.popup_render_elements(renderer, index, projection) {
                elements.push(popup);
            }

            // Selection is part of this window's chrome: higher windows and
            // this window's popups must cover its outline in the same stack.
            if let Some(outline) = group_elements.outlines.remove(&self.windows[index].id) {
                elements.extend(outline);
            }

            let scale = projection.scale;
            let server_side = self.has_compositor_chrome(index);
            let id = self.windows[index].id;
            let active = !self.canvas_preview_active() && self.focused_normal_window_id == Some(id);
            let frame = (self.is_navigable_window(index)
                && !self.windows[index].presentation.fullscreen)
                .then(|| self.normal_window_screen_rect(index))
                .flatten()
                .and_then(|rect| {
                    let kind = if server_side {
                        window_frame::FrameKind::ServerSide
                    } else {
                        let geometry = Rectangle::new(
                            self.window_surface_geometry_loc(index),
                            self.window_geometry_size(index),
                        );
                        self.windows[index]
                            .surface
                            .wl_surface()
                            .map_or(window_frame::FrameKind::UncertainClient, |root| {
                                window_frame::FrameKind::for_client(&root, geometry)
                            })
                    };
                    self.window_frames.elements_with_opacity(
                        renderer,
                        id,
                        rect,
                        scale,
                        active,
                        !self.canvas_preview_active() && hovered_window == Some(id),
                        kind,
                        self.background_image_path.is_some(),
                        self.windows[index].presentation.focused_view,
                        projection.opacity,
                    )
                });
            if let Some(frame) = &frame {
                elements.push(HearthspaceRenderElement::from(frame.rim.clone()));
            }

            // Masonry-rasterized title bar foreground (app identity and controls)
            // layered above the window's content.
            for titlebar in self.titlebar_elements(renderer, index, projection) {
                if let Some(clip) = frame.as_ref().and_then(|frame| frame.clip.as_ref()) {
                    elements.push(HearthspaceRenderElement::from(clip.wrap(titlebar, None)));
                } else {
                    elements.push(HearthspaceRenderElement::from(titlebar));
                }
            }
            if let Some(header) = frame.as_ref().and_then(|frame| frame.header.as_ref()) {
                elements.push(HearthspaceRenderElement::from(header.clone()));
            }

            let origins = match self.windows[index].kind {
                ManagedWindowKind::ShellBar => self.shell_bar_screen_origins(index),
                ManagedWindowKind::Launcher | ManagedWindowKind::ControlOsd => {
                    self.launcher_screen_origins(index)
                }
                ManagedWindowKind::ShellWatcher => Vec::new(),
                ManagedWindowKind::Normal | ManagedWindowKind::DesktopItem => {
                    vec![projection.origin.to_i32_round().to_physical(1)]
                }
            };
            let surface_scale = self.layout_backing_scale(index).unwrap_or(scale.into());
            for origin in origins {
                for surface in
                    self.window_render_elements_at(renderer, index, origin, projection.opacity)
                {
                    let solid = window_frame::solid_color(&surface);
                    let element =
                        RescaleRenderElement::from_element(surface, origin, surface_scale);
                    if let Some(clip) = frame.as_ref().and_then(|frame| frame.clip.as_ref()) {
                        elements.push(HearthspaceRenderElement::from(clip.wrap(element, solid)));
                    } else {
                        elements.push(HearthspaceRenderElement::from(element));
                    }
                }
            }
            if let Some(frame) = frame
                && !self.windows[index].presentation.focused_view
            {
                elements.push(HearthspaceRenderElement::from(frame.shadow));
            }
            let overview = self.is_canvas_map_panel(index);
            let panel = if (self.canvas_search_panel_allowed(index) || overview)
                && let Some(origin) = self.launcher_logical_origin(index)
            {
                let rect = Rectangle::new(
                    origin + self.window_surface_geometry_loc(index),
                    self.window_geometry_size(index),
                );
                Some((
                    rect,
                    1.,
                    !overview
                        && (self.focused_shell_panel_index() == Some(index)
                            || point_in_logical_rect(self.pointer_location, rect)),
                    1.,
                ))
            } else if self.canvas_search_result_visible(index)
                && let Some(pose) = projection.preview
            {
                Some((
                    Rectangle::new(
                        (pose.x.round() as i32, pose.y.round() as i32).into(),
                        (pose.width.round() as i32, pose.height.round() as i32).into(),
                    ),
                    scale,
                    self.canvas_search_result_emphasized(index),
                    projection.opacity,
                ))
            } else {
                None
            };
            if let Some((rect, zoom, emphasized, opacity)) = panel
                && let Some(shadow) = self
                    .window_frames
                    .panel_shadow(renderer, id, rect, zoom, emphasized, opacity)
            {
                elements.push(HearthspaceRenderElement::from(shadow));
            }
            if overview && let Some(origin) = self.launcher_logical_origin(index) {
                window_backdrops.push(window_backdrop::BackdropRequest {
                    id,
                    front,
                    after: elements.len(),
                    window: Rectangle::new(
                        origin + self.window_surface_geometry_loc(index),
                        self.window_geometry_size(index),
                    ),
                    region: None,
                    radius: 20.,
                    modal: false,
                    zoom: 1.,
                    opacity: 1.,
                });
            }
            if self.window_switcher_caption_visible(index)
                && let Some(caption) = self.window_switcher_caption_rect()
                && let Some(shadow) = self.window_frames.rounded_panel_shadow(
                    renderer,
                    id,
                    Rectangle::new(
                        (caption.x.round() as i32, caption.y.round() as i32).into(),
                        (caption.width.round() as i32, caption.height.round() as i32).into(),
                    ),
                    1.,
                    false,
                    1.,
                    10.,
                )
            {
                elements.push(HearthspaceRenderElement::from(shadow));
            }
            if self.windows[index].kind == ManagedWindowKind::Normal
                && !self.windows[index].presentation.fullscreen
                && (self.has_compositor_chrome(index)
                    || super::windows::window_app_id(&self.windows[index].surface).as_deref()
                        == Some(window_backdrop::SETTINGS_APP_ID))
                && let Some(window) = self.normal_window_screen_rect(index)
            {
                window_backdrops.push(window_backdrop::BackdropRequest {
                    id: self.windows[index].id,
                    front,
                    after: elements.len(),
                    window,
                    region: (super::windows::window_app_id(&self.windows[index].surface)
                        .as_deref()
                        != Some(window_backdrop::SETTINGS_APP_ID))
                    .then(|| self.title_bar_rect(index)),
                    radius: if self.windows[index].presentation.focused_view {
                        0.
                    } else {
                        19.
                    },
                    modal: false,
                    zoom: scale,
                    opacity: projection.opacity,
                });
            }
            if let Some(rect) = self.layout_glass_rect(index) {
                let radius = self.layout_glass_radius(index);
                let emphasized = self.frame_group_emphasized(index)
                    || self.focused_shell_panel_index() == Some(index)
                    || point_in_logical_rect(self.pointer_location, rect);
                if let Some(shadow) = self.window_frames.rounded_panel_shadow(
                    renderer,
                    id,
                    rect,
                    scale,
                    emphasized,
                    1.,
                    radius as f32,
                ) {
                    elements.push(HearthspaceRenderElement::from(shadow));
                }
                window_backdrops.push(window_backdrop::BackdropRequest {
                    id,
                    front,
                    after: elements.len(),
                    window: rect,
                    region: None,
                    radius: (radius - 1.).max(0.),
                    modal: false,
                    zoom: scale,
                    opacity: 1.,
                });
            }
            if let Some(clip) = self.layout_window_clip(index) {
                layout_clip::clip_elements(&mut elements, front, clip);
                for request in window_backdrops
                    .iter_mut()
                    .filter(|request| request.id == id)
                {
                    request.after = elements.len();
                    request.region = Some(
                        request
                            .region
                            .unwrap_or(request.window)
                            .intersection(clip)
                            .unwrap_or_else(|| Rectangle::from_size((0, 0).into())),
                    );
                }
            }
            window_spans.push((id, front..elements.len()));
            if self.modal_confirmation_surface(index) && !layout.modal.is_empty() {
                let front = elements.len();
                elements.append(&mut layout.modal);
                window_backdrops.push(window_backdrop::BackdropRequest {
                    id: u64::MAX,
                    front,
                    after: elements.len(),
                    window: output_rect,
                    region: None,
                    radius: 0.,
                    modal: true,
                    zoom: 1.,
                    opacity: 1.,
                });
            }
            // Fullscreen content must not reveal unrelated windows through
            // transparent pixels or around a smaller-than-requested buffer.
            if self.windows[index].presentation.fullscreen && !self.canvas_preview_presenting() {
                break;
            }
        }

        append_layout_outlines(
            &mut elements,
            &mut layout.front,
            &mut layout.owners,
            &mut window_spans,
        );
        let desktop_start = desktop_start.unwrap_or(elements.len());
        elements.append(&mut group_elements.overlay);
        elements.extend(layout.back);
        RenderScene {
            elements,
            desktop_start,
            window_backdrops,
            window_spans,
        }
    }

    /// Build the cursor render elements for the current frame, front-to-back.
    ///
    /// The software cursor is only used when the active backend has opted into
    /// it (`software_cursor_visible`): headless always, the native backend as a
    /// fallback when the hardware cursor plane is unavailable, and either
    /// backend whenever a focused client is drawing its own cursor surface. A
    /// client cursor surface takes precedence over the compositor's named shape
    /// and is positioned by its own committed hotspot. Both paths are built at
    /// native scale; the surrounding [`render_frame_at`](Self::render_frame_at)
    /// rescales the whole scene by the output scale, so the cursor scales with
    /// monitor DPI and its hotspot always lands on the logical pointer position.
    pub(super) fn software_cursor_elements(
        &mut self,
        renderer: &mut GlesRenderer,
        output_rect: Rectangle<i32, Logical>,
    ) -> Vec<HearthspaceRenderElement> {
        if !self.software_cursor_visible || !self.cursor_visible {
            return Vec::new();
        }
        if !point_in_logical_rect(self.pointer_location, output_rect) {
            return Vec::new();
        }

        if let Some(surface) = self.cursor_surface.clone() {
            if surface.alive() {
                let hotspot = with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<Mutex<CursorImageAttributes>>()
                        .map(|attributes| {
                            attributes
                                .lock()
                                .expect("cursor image attributes poisoned")
                                .hotspot
                        })
                        .unwrap_or_default()
                });
                let location = Point::<i32, Physical>::from((
                    self.pointer_location.x.round() as i32 - hotspot.x,
                    self.pointer_location.y.round() as i32 - hotspot.y,
                ));
                let surface_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                    render_elements_from_surface_tree(
                        renderer,
                        &surface,
                        location,
                        1.0,
                        1.0,
                        Kind::Cursor,
                    );
                return surface_elements
                    .into_iter()
                    .map(|element| {
                        HearthspaceRenderElement::from(RescaleRenderElement::from_element(
                            element, location, 1.0,
                        ))
                    })
                    .collect();
            }
            // The client destroyed its cursor surface; drop it and fall back to
            // the compositor's named cursor below.
            self.cursor_surface = None;
        }

        // Named shape: rebuild the cursor buffer only when the desired icon
        // changes, so steady-state motion reuses the cached rasterization.
        if self.software_cursor.icon != self.cursor_icon {
            self.software_cursor =
                crate::compositor::cursor::software_cursor_for_icon(self.cursor_icon);
        }

        let location = Point::<f64, Physical>::from((
            self.pointer_location.x.round() - f64::from(self.software_cursor.hotspot.0),
            self.pointer_location.y.round() - f64::from(self.software_cursor.hotspot.1),
        ));

        MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            location,
            &self.software_cursor.buffer,
            None,
            None,
            None,
            Kind::Cursor,
        )
        .ok()
        .map(HearthspaceRenderElement::from)
        .into_iter()
        .collect()
    }

    /// Collect render elements for every popup anchored to the given window.
    ///
    /// Popups (xdg_popup surfaces such as menus) are not part of the toplevel's
    /// surface tree, so they are gathered separately and positioned relative to
    /// the parent surface using the popup's configured location, mirroring the
    /// math in Smithay's own `Window` rendering. They share the window's render
    /// scale so the viewport zoom applies uniformly.
    fn popup_render_elements(
        &self,
        renderer: &mut GlesRenderer,
        window_index: usize,
        projection: super::presentation::Projection,
    ) -> Vec<HearthspaceRenderElement> {
        let window = &self.windows[window_index];
        let Some(parent) = window.surface.wl_surface() else {
            return Vec::new();
        };
        let base = if self.windows[window_index].kind.is_shell_chrome() {
            self.surface_screen_origin(window_index)
        } else {
            projection.origin.to_i32_round().to_physical(1)
        };
        let scale = projection.scale;
        let geometry_loc = self.window_surface_geometry_loc(window_index);

        let mut elements = Vec::new();
        for (popup, popup_offset) in PopupManager::popups_for_surface(&parent) {
            // Offset of the popup surface origin from the parent surface origin,
            // in the parent's native (unscaled) logical coordinates.
            let offset = geometry_loc + popup_offset - popup.geometry().loc;
            let offset_physical = Point::<i32, Physical>::from((
                (f64::from(offset.x) * scale).round() as i32,
                (f64::from(offset.y) * scale).round() as i32,
            ));
            let popup_origin = base + offset_physical;

            for surface in
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    popup.wl_surface(),
                    popup_origin,
                    1.0,
                    1.0,
                    Kind::Unspecified,
                )
            {
                elements.push(HearthspaceRenderElement::from(
                    RescaleRenderElement::from_element(surface, popup_origin, scale),
                ));
            }
        }
        elements
    }

    fn window_render_elements_at(
        &self,
        renderer: &mut GlesRenderer,
        window_index: usize,
        origin: Point<i32, Physical>,
        opacity: f32,
    ) -> Vec<WaylandSurfaceRenderElement<GlesRenderer>> {
        let window = &self.windows[window_index];
        let Some(root) = window.surface.wl_surface() else {
            return Vec::new();
        };
        render_elements_from_surface_tree(
            renderer,
            &root,
            origin,
            // Built at native scale; the viewport zoom is applied by wrapping
            // these elements in a `RescaleRenderElement` in the caller.
            1.0,
            opacity,
            Kind::Unspecified,
        )
    }

    pub(super) fn window_render_scale(&self, window_index: usize) -> f64 {
        self.window_projection(window_index).scale
    }
}

fn append_layout_outlines(
    elements: &mut Vec<HearthspaceRenderElement>,
    outlines: &mut Vec<HearthspaceRenderElement>,
    owners: &mut Vec<(usize, u64)>,
    spans: &mut Vec<(u64, std::ops::Range<usize>)>,
) {
    for (index, id) in std::mem::take(owners) {
        let start = elements.len() + index;
        spans.push((id, start..start + 1));
    }
    elements.append(outlines);
}

fn point_in_logical_rect(point: Point<f64, Logical>, rect: Rectangle<i32, Logical>) -> bool {
    point.x >= f64::from(rect.loc.x)
        && point.x < f64::from(rect.loc.x + rect.size.w.max(1))
        && point.y >= f64::from(rect.loc.y)
        && point.y < f64::from(rect.loc.y + rect.size.h.max(1))
}

/// The offset of a toplevel's window geometry within its surface, used to place
/// popups relative to the parent surface origin. Defaults to `(0, 0)` when the
/// client has not set an explicit window geometry.
pub(super) fn toplevel_geometry_loc(surface: &wl_surface::WlSurface) -> Point<i32, Logical> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|geometry| geometry.loc)
            .unwrap_or_default()
    })
}

/// The size of a toplevel's window geometry (the visible window excluding any
/// client-side decoration shadow margins). `None` when the client has not set
/// an explicit window geometry, in which case the caller falls back to the
/// surface-tree bounding box.
pub(super) fn toplevel_geometry_size(
    surface: &wl_surface::WlSurface,
) -> Option<Size<i32, Logical>> {
    with_states(surface, |states| {
        states
            .cached_state
            .get::<SurfaceCachedState>()
            .current()
            .geometry
            .map(|geometry| geometry.size)
    })
}

pub(super) fn send_frames_surface_tree(surface: &wl_surface::WlSurface, time: u32) {
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_surf, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

#[cfg(test)]
mod tests {
    use smithay::utils::{Logical, Point, Rectangle};

    use super::point_in_logical_rect;

    #[test]
    fn point_in_logical_rect_excludes_other_outputs() {
        let rect = Rectangle::<i32, Logical>::new((1280, 0).into(), (1024, 768).into());

        assert!(point_in_logical_rect(Point::from((1280.0, 12.0)), rect));
        assert!(point_in_logical_rect(Point::from((2303.0, 767.0)), rect));
        assert!(!point_in_logical_rect(Point::from((1279.0, 12.0)), rect));
        assert!(!point_in_logical_rect(Point::from((2304.0, 12.0)), rect));
    }
}
