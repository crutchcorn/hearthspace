//! Optional copy route. The canonical scene texture and Smithay fallback remain
//! authoritative; no foreign context ever receives its raw GLES texture handle.
mod draw;
mod setup;
mod slot;

use super::OutputPass;
use crate::compositor::udev::gpu::{
    GpuApi, GpuState,
    bridge_policy::{Attempt, direct_format},
};
pub(in crate::compositor::udev) use slot::Slot;
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf},
        drm::DrmNode,
        renderer::{
            Bind, ImportDma,
            gles::{GlesRenderer, GlesTexture},
            multigpu::{ApiDevice, GpuManager},
            sync::SyncPoint,
        },
    },
    utils::{Physical, Size},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub(super) struct Request<'a> {
    pub source: DrmNode,
    pub target: DrmNode,
    pub output: &'a str,
    pub size: Size<i32, Physical>,
    pub copy_format: Fourcc,
    pub scene: &'a GlesTexture,
    pub scene_sync: &'a SyncPoint,
    pub dmabuf: &'a mut Dmabuf,
    pub capture: bool,
}

pub(super) fn direct_copy_format(
    manager: &mut GpuManager<GpuApi>,
    source: DrmNode,
    target: DrmNode,
    preferred: Fourcc,
) -> Result<Option<Fourcc>> {
    let mut render = Vec::new();
    let mut import = Vec::new();
    for device in manager.devices_mut()? {
        if *device.node() == source && device.should_do_cross_device_exports() {
            render = <GlesRenderer as Bind<Dmabuf>>::supported_formats(device.renderer())
                .unwrap_or_default()
                .into_iter()
                .collect();
        }
        if *device.node() == target && device.can_do_cross_device_imports() {
            import = device.renderer().dmabuf_formats().into_iter().collect();
        }
    }
    Ok(direct_format(preferred, &render, &import))
}

pub(super) fn render(gpus: &mut GpuState, mut request: Request<'_>) -> Result<Option<OutputPass>> {
    gpus.bridges.poll();
    let key = match setup::key(gpus, &request) {
        Ok(key) => key,
        Err(_) => return Ok(None), // unavailable renderer/incarnation: Smithay owns fallback
    };
    gpus.bridges.replace_output(&key);
    if !gpus.bridges.permits(&key) {
        return Ok(None);
    }
    if gpus.bridges.active(&key).is_none() {
        if !gpus.bridges.has_room() {
            return Ok(None);
        }
        let created = setup::create(&mut gpus.manager, &key);
        match created {
            Ok(slot) => {
                if !gpus.bridges.insert(key.clone(), slot) {
                    return Ok(None);
                }
            }
            Err(error) => {
                tracing::warn!(%error, output = key.output, "Vulkan transfer setup unavailable; using Smithay fallback");
                gpus.bridges.fail(&key);
                return Attempt::default().finish(Err(error));
            }
        }
    }
    let slot = gpus.bridges.active(&key).expect("admitted bridge slot");
    let mut attempt = Attempt::default();
    let result: Result<Option<OutputPass>> = (|| {
        let work = slot.resources.as_mut().expect("live slot resources");
        // Capacity pressure is temporary. The canonical texture is independent
        // of these pending allocations, so Smithay can serve this frame safely.
        if !work.bridge.as_mut().expect("active bridge").ready()?
            || work.holds.len() == slot::EXTERNAL_LIMIT
        {
            return Ok(None);
        }
        // Keep every sampled scene and written scanout alive across output
        // destruction, including commands whose completion export might fail.
        work.holds
            .push((request.scene.clone(), request.dmabuf.clone()));
        attempt.begin_gpu();
        slot.complete = false;
        let pass = draw::copy(&mut gpus.manager, work, &mut request)?;
        slot.poll()?;
        Ok(Some(pass))
    })();
    if let Err(error) = &result {
        tracing::warn!(%error, output = key.output, "retiring Vulkan transfer attempt");
        gpus.bridges.fail(&key);
    }
    attempt.finish(result).map(Option::flatten)
}
