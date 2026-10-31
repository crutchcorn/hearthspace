mod boundary;
mod frame;
mod memory;
mod renderer;

pub use boundary::{Api, ManualFence, SharedStats};
pub use renderer::FORMAT;

use crate::output_transfer::OutputTransfer;
use boundary::{Device, Stats, buffer, nodes};
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::WeakDmabuf},
        drm::DrmNode,
        renderer::{
            Bind, Color32F, ExportMem, Frame, Renderer,
            multigpu::{GpuManager, TransferCache},
            sync::SyncPoint,
        },
    },
    utils::{Physical, Rectangle, Size, Transform},
};
use std::{
    cell::RefCell,
    io,
    sync::{Arc, Mutex},
};

pub const DP8: (i32, i32) = (3840, 2160);
pub const DP9: (i32, i32) = (2560, 3200);

pub struct Fixture {
    pub manager: GpuManager<Api>,
    pub source: DrmNode,
    pub target: DrmNode,
    pub stats: SharedStats,
}

pub struct Submitted {
    pub sync: SyncPoint,
    pub readback: Result<(), io::Error>,
}

impl Fixture {
    pub fn new() -> Self {
        let stats = Arc::new(Mutex::new(Stats::default()));
        let (source, target) = nodes();
        let devices = vec![
            Device::new(source, true, stats.clone()),
            Device::new(target, false, stats.clone()),
        ];
        Self {
            manager: GpuManager::new(Api(RefCell::new(devices))).unwrap(),
            source,
            target,
            stats,
        }
    }

    /// Exercise the public renderer API, observing the allocation actually used
    /// for source rendering and target sampling. No private cache helper runs here.
    pub fn draw(
        &mut self,
        cache: Option<&mut TransferCache>,
        size: (i32, i32),
        format: Fourcc,
    ) -> WeakDmabuf {
        let size: Size<i32, Physical> = size.into();
        let mut scanout = buffer((size.w, size.h).into(), FORMAT);
        let renderer = self
            .manager
            .renderer(&self.source, &self.target, format)
            .unwrap();
        let mut renderer = if let Some(cache) = cache {
            renderer.with_transfer_cache(cache)
        } else {
            renderer
        };
        let mut framebuffer = renderer.bind(&mut scanout).unwrap();
        let mut frame = renderer
            .render(&mut framebuffer, size, Transform::Normal)
            .unwrap();
        frame
            .clear(Color32F::TRANSPARENT, &[Rectangle::from_size(size)])
            .unwrap();
        frame.finish().unwrap().wait().unwrap();
        drop(framebuffer);
        drop(renderer);
        self.last_intermediate()
    }

    /// Use Hearthspace's actual fence owner/finish helper with a real MultiFrame.
    /// Readback and submission failures remain outside that owner's lifetime.
    pub fn draw_owned(
        &mut self,
        owner: &mut OutputTransfer,
        size: (i32, i32),
        capture: bool,
    ) -> Result<Submitted, Box<dyn std::error::Error>> {
        {
            let mut renderer = self.manager.single_renderer(&self.source)?;
            owner.prepare(renderer.as_mut())?;
        }
        let size: Size<i32, Physical> = size.into();
        let mut scanout = buffer((size.w, size.h).into(), FORMAT);
        let mut renderer = self
            .manager
            .renderer(&self.source, &self.target, FORMAT)?
            .with_transfer_cache(&mut owner.cache);
        let mut framebuffer = renderer.bind(&mut scanout)?;
        let completed = (|| {
            let frame = renderer.render(&mut framebuffer, size, Transform::Normal)?;
            owner.completion.finish(frame, |frame| {
                frame.clear(Color32F::TRANSPARENT, &[Rectangle::from_size(size)])
            })
        })();
        let (drawn, sync) = match completed {
            Ok(result) => result,
            Err(error) => {
                drop(framebuffer);
                renderer.invalidate_caches().unwrap();
                drop(renderer);
                owner.reset();
                return Err(error.into());
            }
        };
        drawn?;
        let readback = if capture {
            renderer
                .target_as_mut()
                .unwrap()
                .copy_framebuffer(
                    framebuffer.as_ref(),
                    Rectangle::from_size((size.w, size.h).into()),
                    FORMAT,
                )
                .map(|_| ())
        } else {
            Ok(())
        };
        Ok(Submitted { sync, readback })
    }

    pub fn edit_device(&mut self, node: DrmNode, edit: impl FnOnce(&mut Device)) {
        let device = self
            .manager
            .devices_mut()
            .unwrap()
            .find(|device| device.node == node)
            .unwrap();
        edit(device);
    }

    pub fn last_intermediate(&self) -> WeakDmabuf {
        self.stats
            .lock()
            .unwrap()
            .source_writes
            .last()
            .unwrap()
            .clone()
    }

    pub fn last_fence(&self) -> ManualFence {
        self.stats
            .lock()
            .unwrap()
            .completions
            .last()
            .unwrap()
            .clone()
    }

    pub fn allocations(&self) -> usize {
        self.stats.lock().unwrap().allocations.len()
    }
}
