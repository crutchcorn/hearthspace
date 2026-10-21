use super::App;
impl App {
    pub(in crate::compositor) fn advance_canvas_search(&mut self) {
        if let Some(session) = self.canvas_search.session.as_ref() {
            let output = self
                .output_logical_rect_for_name(session.output.as_deref())
                .map(super::scene::rect);
            match output {
                None => self.close_canvas_search(true),
                Some(view) if view != session.view => {
                    let s = self.canvas_search.session.as_mut().unwrap();
                    s.anchor.x =
                        (s.anchor.x - s.view.x + view.x).clamp(view.x, view.x + view.width);
                    s.anchor.y =
                        (s.anchor.y - s.view.y + view.y).clamp(view.y, view.y + view.height);
                    s.view = view;
                    s.signature = None;
                }
                _ => {}
            }
        }
        if self.canvas_search_active() {
            self.rebuild_canvas_search();
            self.refresh_canvas_search_hover();
        }
    }
}
