//! Background thumbnails are prepared off-thread and retained across page changes.
use xilem::{masonry::peniko::ImageBrush, tokio::sync::watch};

mod view;
mod worker;

pub(crate) use view::view;
pub(crate) use worker::task;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    generation: u64,
    path: Option<String>,
}

#[derive(Debug)]
struct Response {
    request: Request,
    image: Option<ImageBrush>,
}

#[derive(Default)]
pub(crate) struct BackgroundPreview {
    request: Request,
    sender: Option<watch::Sender<Request>>,
    image: Option<ImageBrush>,
}

impl BackgroundPreview {
    /// Compare paths only: filesystem work belongs exclusively to the worker.
    pub(crate) fn synchronize_path(&mut self, path: Option<&str>) {
        if self.request.path.as_deref() != path {
            self.refresh(path);
        }
    }

    /// Recheck source metadata after selection or returning to Background.
    pub(crate) fn refresh(&mut self, path: Option<&str>) {
        if self.request.path.as_deref() != path || path.is_none() {
            self.image = None;
        }
        self.request = Request {
            generation: self.request.generation.wrapping_add(1),
            path: path.map(str::to_owned),
        };
        if let Some(sender) = &self.sender {
            sender.send_replace(self.request.clone());
        }
    }

    fn receive(&mut self, response: Response) {
        if self.request == response.request {
            self.image = response.image;
        }
    }

    fn image_for(&self, path: Option<&str>) -> Option<ImageBrush> {
        if self.request.path.as_deref() == path {
            self.image.clone()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests;
