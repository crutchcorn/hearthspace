//! Internal SHM content edges isolate blur tests from window decorations.
use super::{MapClient, QueueHandle, State, Window, wl_shm};
use std::{io::Write, os::fd::AsFd};

impl State {
    pub(super) fn commit_buffer(&mut self, number: usize, handle: &QueueHandle<Self>) {
        let window = &self.windows[number];
        if !window.alive {
            return;
        }
        let width = window.size.0 + window.inset.0 * 2;
        let height = window.size.1 + window.inset.1 * 2;
        let size = width as usize * height as usize * 4;
        let mut file = tempfile::tempfile().unwrap();
        file.set_len(size as u64).unwrap();
        file.write_all(&paint(window)).unwrap();
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), size as i32, handle, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Argb8888,
            handle,
            (),
        );
        window.xdg_surface.set_window_geometry(
            window.inset.0,
            window.inset.1,
            window.size.0,
            window.size.1,
        );
        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage_buffer(0, 0, width, height);
        window.surface.commit();
        pool.destroy();
        self.buffers.push((file, buffer));
    }
}

pub(super) struct ColoredRectangle {
    bounds: (i32, i32, i32, i32),
    color: [u8; 4],
}

impl MapClient {
    /// Coordinates are relative to the client geometry, excluding its inset.
    /// Retain the pattern when configure events require another buffer commit.
    pub(in super::super) fn repaint_rectangle(
        &mut self,
        number: usize,
        background: [u8; 4],
        bounds: (i32, i32, i32, i32),
        color: [u8; 4],
    ) {
        let window = &mut self.state.windows[number];
        window.color = background;
        window.rectangle = Some(ColoredRectangle { bounds, color });
        self.state.commit_buffer(number, &self.queue.handle());
        self.sync();
    }
}

pub(super) fn paint(window: &Window) -> Vec<u8> {
    let width = window.size.0 + window.inset.0 * 2;
    let height = window.size.1 + window.inset.1 * 2;
    let mut pixels = window.color.repeat(width as usize * height as usize);
    if let Some(rectangle) = &window.rectangle {
        let (x, y, w, h) = rectangle.bounds;
        let left = x.clamp(0, window.size.0);
        let top = y.clamp(0, window.size.1);
        let right = x.saturating_add(w).clamp(left, window.size.0);
        let bottom = y.saturating_add(h).clamp(top, window.size.1);
        for row in top..bottom {
            let start = ((row + window.inset.1) * width + left + window.inset.0) as usize * 4;
            let end = start + (right - left) as usize * 4;
            for pixel in pixels[start..end].as_chunks_mut::<4>().0 {
                pixel.copy_from_slice(&rectangle.color);
            }
        }
    }
    pixels
}
