//! Opt-in native touch observations for shell modal delivery tests.
use super::*;
use wayland_client::protocol::wl_touch;

#[derive(Default)]
pub(super) struct State {
    down: Vec<(usize, (f64, f64))>,
    cancelled: usize,
}

impl MapClient {
    pub(in super::super) fn enable_touch(&mut self) {
        self.state
            .dnd
            .seat
            .as_ref()
            .unwrap()
            .get_touch(&self.queue.handle(), ());
        self.sync();
    }

    pub(in super::super) fn touches(&mut self) -> &[(usize, (f64, f64))] {
        self.sync();
        &self.state.touch.down
    }

    pub(in super::super) fn touch_cancellations(&mut self) -> usize {
        self.sync();
        self.state.touch.cancelled
    }
}

impl Dispatch<wl_touch::WlTouch, ()> for super::State {
    fn event(
        state: &mut Self,
        _: &wl_touch::WlTouch,
        event: wl_touch::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_touch::Event::Down { surface, x, y, .. } => {
                if let Some(index) = state.windows.iter().position(|w| w.surface == surface) {
                    state.touch.down.push((index, (x, y)));
                }
            }
            wl_touch::Event::Cancel => state.touch.cancelled += 1,
            _ => {}
        }
    }
}
