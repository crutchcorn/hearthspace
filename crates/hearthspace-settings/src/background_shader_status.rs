//! Render wallpaper thumbnails through the desktop's shader renderer.
use crate::state::SettingsApp;
use std::{path::Path, time::Duration};
use xilem::{
    ViewCtx,
    core::{NoElement, View},
    masonry::peniko::{ImageAlphaType, ImageBrush, ImageData, ImageFormat},
    tokio::sync::watch,
    view::task_raw,
};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Request {
    path: Option<String>,
    generation: u64,
    reload: bool,
}
#[derive(Default)]
pub(crate) struct ShaderStatus {
    request: Request,
    sender: Option<watch::Sender<Request>>,
    pub(crate) image: Option<ImageBrush>,
}
impl ShaderStatus {
    pub(crate) fn synchronize_path(&mut self, path: Option<&str>) {
        if self.request.path.as_deref() != path {
            self.request.path = path.map(str::to_owned);
            self.image = None;
            self.request.generation = self.request.generation.wrapping_add(1);
            self.request.reload = false;
            if let Some(sender) = &self.sender {
                sender.send_replace(self.request.clone());
            }
        }
    }
    fn receive(&mut self, request: Request, image: Option<ImageBrush>) {
        if self.request == request && image.is_some() {
            self.image = image;
        }
    }
    pub(crate) fn reload(&mut self) {
        self.request.reload = true;
        self.request.generation = self.request.generation.wrapping_add(1);
        if let Some(sender) = &self.sender {
            sender.send_replace(self.request.clone());
        }
    }
}

pub(crate) fn load_preview(path: &Path) -> Result<ImageBrush, String> {
    let socket =
        crate::settings_backend::command_socket_path().map_err(|_| "Wallpaper failed to load")?;
    let path = path.to_str().ok_or("Wallpaper failed to load")?;
    let command = format!(
        "wallpaper-preview {}",
        serde_json::to_string(path).map_err(|_| "Wallpaper failed to load")?
    );
    let png = hearthspace_ipc::transport::request_payload(
        &socket,
        &command,
        Duration::from_secs(3),
        2 * 1024 * 1024,
    )
    .map_err(|_| "Wallpaper failed to load")?;
    let pixels = image::load_from_memory(&png)
        .map_err(|_| "Wallpaper failed to load")?
        .into_rgba8();
    let (width, height) = pixels.dimensions();
    Ok(ImageData {
        data: pixels.into_raw().into(),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width,
        height,
    }
    .into())
}

pub(crate) fn task() -> impl View<SettingsApp, (), ViewCtx, Element = NoElement> + use<> {
    task_raw(
        |proxy, state: &mut SettingsApp| {
            let (sender, mut receiver) =
                watch::channel(state.background_shader_status.request.clone());
            state.background_shader_status.sender = Some(sender);
            async move {
                loop {
                    let request = receiver.borrow_and_update().clone();
                    if let Some(path) = request.path.clone() {
                        let reload = request.reload;
                        let result = crate::background_preview::blocking_load(move || {
                            if reload {
                                let socket = crate::settings_backend::command_socket_path()
                                    .map_err(|_| "Wallpaper failed to load".to_string())?;
                                hearthspace_ipc::transport::request_line(
                                    &socket,
                                    "reload-settings\n",
                                    Duration::from_secs(3),
                                )
                                .map_err(|_| "Wallpaper failed to load".to_string())?;
                            }
                            load_preview(Path::new(&path))
                        })
                        .await;
                        if proxy
                            .message((request, result.and_then(Result::ok)))
                            .is_err()
                        {
                            break;
                        }
                    }
                    if receiver.changed().await.is_err() {
                        break;
                    }
                }
            }
        },
        |state: &mut SettingsApp, (request, image)| {
            if state.background_shader_status.request == request && image.is_none() {
                state.status = "Wallpaper failed to load".into();
            }
            state.background_shader_status.receive(request, image);
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn late_shader_preview_cannot_replace_a_new_selection_or_reset() {
        let mut status = ShaderStatus::default();
        status.synchronize_path(Some("first.glsl"));
        let first = status.request.clone();
        status.synchronize_path(Some("second.glsl"));
        let image: ImageBrush = ImageData {
            data: vec![0, 0, 255, 255].into(),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        }
        .into();
        status.receive(first, Some(image.clone()));
        assert!(status.image.is_none());
        let second = status.request.clone();
        status.receive(second.clone(), Some(image.clone()));
        assert_eq!(
            status.image.as_ref().unwrap().image.data.as_ref(),
            &[0, 0, 255, 255]
        );
        status.receive(second.clone(), None);
        assert!(
            status.image.is_some(),
            "failed refresh retains the last rendered thumbnail"
        );
        status.synchronize_path(None);
        status.receive(second, Some(image));
        assert!(status.image.is_none());
    }
}
