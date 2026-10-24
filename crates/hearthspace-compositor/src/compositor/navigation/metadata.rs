use hearthspace_ipc::windows::{Bounds, Window, WindowList};
use smithay::utils::Rectangle;

use super::super::{
    App, ManagedSurface,
    windows::{window_app_id, window_title},
};

impl App {
    pub(in crate::compositor) fn window_list_json(&self) -> Vec<u8> {
        serde_json::to_vec(&WindowList {
            version: 1,
            windows: self.automation_windows(),
        })
        .expect("serializable window list")
    }

    pub(in crate::compositor) fn automation_windows(&self) -> Vec<Window> {
        let outputs = self.capture_outputs();
        self.windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                if !self.is_navigable_window(index) {
                    return None;
                }
                let rect = self.normal_window_screen_rect(index)?;
                let visible_outputs: Vec<_> = outputs
                    .iter()
                    .filter(|output| {
                        let bounds = Rectangle::new(
                            (output.logical_x, output.logical_y).into(),
                            (output.logical_width, output.logical_height).into(),
                        );
                        rect.overlaps(bounds)
                    })
                    .map(|output| output.name.clone())
                    .collect();
                let content = self.window_geometry_size(index);
                let client_type = match &window.surface {
                    ManagedSurface::Xdg(_) => "wayland",
                    #[cfg(feature = "xwayland")]
                    ManagedSurface::X11(_) => "x11",
                };
                Some(Window {
                    id: window.id,
                    title: window_title(&window.surface),
                    app_id: window_app_id(&window.surface),
                    x: rect.loc.x,
                    y: rect.loc.y,
                    width: rect.size.w,
                    height: rect.size.h,
                    titlebar_height: if self.has_compositor_chrome(index) {
                        self.title_bar_rect(index).size.h
                    } else {
                        0
                    },
                    canvas_x: window.position.x,
                    canvas_y: window.position.y,
                    content_width: content.w,
                    content_height: content.h,
                    mapped: window.mapped,
                    visible: !visible_outputs.is_empty(),
                    outputs: visible_outputs,
                    focused: self.focused_normal_window_id == Some(window.id),
                    client_type: client_type.into(),
                    fullscreen: window.presentation.fullscreen,
                    maximized: window.presentation.maximized,
                    focused_view: window.presentation.focused_view,
                    maximized_canvas_bounds: window.presentation.maximized_bounds.map(|bounds| {
                        Bounds {
                            x: bounds.loc.x,
                            y: bounds.loc.y,
                            width: bounds.size.w,
                            height: bounds.size.h,
                        }
                    }),
                    output_coordinates: self.window_uses_output_coordinates(index),
                    presentation_output: window
                        .presentation
                        .output
                        .as_ref()
                        .filter(|_| window.presentation.output_pinned())
                        .cloned(),
                })
            })
            .collect()
    }
}
