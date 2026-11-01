//! GPU/device substitutes only. Smithay owns cache selection and both copy paths.
use smithay::{
    backend::{
        allocator::{
            Allocator, Buffer, Fourcc, Modifier,
            dmabuf::{AnyError, Dmabuf, DmabufAllocator, DmabufFlags, WeakDmabuf},
        },
        drm::DrmNode,
        renderer::{
            ContextId, Texture,
            multigpu::{ApiDevice, GraphicsApi},
            sync::{Fence, Interrupted},
        },
    },
    utils::{Buffer as BufferCoords, Size},
};
use std::{
    cell::RefCell,
    io,
    os::fd::OwnedFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use super::renderer::DeviceRenderer;

pub type SharedStats = Arc<Mutex<Stats>>;

#[derive(Debug, Default)]
pub struct Stats {
    pub allocations: Vec<WeakDmabuf>,
    pub source_writes: Vec<WeakDmabuf>,
    pub dma_imports: Vec<WeakDmabuf>,
    pub direct_samples: Vec<WeakDmabuf>,
    pub downloads: usize,
    pub uploads: usize,
    pub memory_samples: usize,
    pub completions: Vec<ManualFence>,
}

#[derive(Debug, Default, Clone)]
pub struct ManualFence {
    pub interrupted: Arc<AtomicBool>,
    pub waits: Arc<AtomicUsize>,
    reached: Arc<AtomicBool>,
}

impl Fence for ManualFence {
    fn is_signaled(&self) -> bool {
        self.reached.load(Ordering::SeqCst)
    }

    fn wait(&self) -> Result<(), Interrupted> {
        self.waits.fetch_add(1, Ordering::SeqCst);
        if self.interrupted.load(Ordering::SeqCst) {
            return Err(Interrupted);
        }
        self.reached.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn is_exportable(&self) -> bool {
        false
    }

    fn export(&self) -> Option<OwnedFd> {
        None
    }
}

/// Each allocation owns a distinct file and Dmabuf identity. Nothing opens DRM.
pub fn buffer(size: Size<i32, BufferCoords>, format: Fourcc) -> Dmabuf {
    let file: OwnedFd = tempfile::tempfile().unwrap().into();
    let mut builder = Dmabuf::builder(size, format, Modifier::Linear, DmabufFlags::empty());
    assert!(builder.add_plane(file, 0, size.w as u32 * 4));
    builder.build().unwrap()
}

#[derive(Debug)]
pub struct CountingAllocator(pub SharedStats);

impl Allocator for CountingAllocator {
    type Buffer = Dmabuf;
    type Error = io::Error;

    fn create_buffer(
        &mut self,
        width: u32,
        height: u32,
        fourcc: Fourcc,
        modifiers: &[Modifier],
    ) -> Result<Dmabuf, io::Error> {
        assert!(modifiers.contains(&Modifier::Linear));
        let dmabuf = buffer((width as i32, height as i32).into(), fourcc);
        self.0.lock().unwrap().allocations.push(dmabuf.weak());
        Ok(dmabuf)
    }
}

#[derive(Debug, Clone)]
pub struct Image {
    pub size: Size<i32, BufferCoords>,
    pub format: Fourcc,
    pub buffer: Option<Dmabuf>,
    pub pixels: Vec<u8>,
}

impl Image {
    pub fn from_buffer(buffer: &Dmabuf) -> Self {
        Self {
            size: buffer.size(),
            format: buffer.format().code,
            buffer: Some(buffer.clone()),
            pixels: Vec::new(),
        }
    }
}

impl Texture for Image {
    fn width(&self) -> u32 {
        self.size.w as u32
    }

    fn height(&self) -> u32 {
        self.size.h as u32
    }

    fn format(&self) -> Option<Fourcc> {
        Some(self.format)
    }
}

#[derive(Debug)]
pub struct Device {
    pub node: DrmNode,
    pub renderer: DeviceRenderer,
    allocator: DmabufAllocator<CountingAllocator>,
}

impl Device {
    pub fn new(node: DrmNode, source: bool, stats: SharedStats) -> Self {
        Self {
            node,
            renderer: DeviceRenderer::new(source, stats.clone()),
            allocator: DmabufAllocator(CountingAllocator(stats)),
        }
    }

    pub fn replace_context(&mut self) {
        let replacement = DeviceRenderer::new(self.renderer.source, self.renderer.stats.clone());
        assert_ne!(self.renderer.context, replacement.context);
        self.renderer = replacement;
    }
}

impl ApiDevice for Device {
    type Renderer = DeviceRenderer;

    fn renderer(&self) -> &DeviceRenderer {
        &self.renderer
    }

    fn renderer_mut(&mut self) -> &mut DeviceRenderer {
        &mut self.renderer
    }

    fn allocator(&mut self) -> &mut dyn Allocator<Buffer = Dmabuf, Error = AnyError> {
        &mut self.allocator
    }

    fn node(&self) -> &DrmNode {
        &self.node
    }

    fn can_do_cross_device_imports(&self) -> bool {
        true
    }
}

#[derive(Debug)]
pub struct Api(pub RefCell<Vec<Device>>);

impl GraphicsApi for Api {
    type Device = Device;
    type Error = io::Error;

    fn enumerate(&self, list: &mut Vec<Device>) -> Result<(), io::Error> {
        list.extend(self.0.borrow_mut().drain(..));
        Ok(())
    }

    fn identifier() -> &'static str {
        "transfer-cache-device-boundary"
    }
}

pub fn nodes() -> (DrmNode, DrmNode) {
    // drm-rs validates even from_dev_id against sysfs. Borrow identities only;
    // these devices are never opened. Primary/render nodes of one GPU suffice
    // because the fixture supplies independent renderers at the device boundary.
    let mut nodes = std::fs::read_dir("/sys/class/drm")
        .expect("transfer-cache fixture requires read-only DRM sysfs identities")
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("dev")).ok())
        .filter_map(|dev| {
            let (major, minor) = dev.trim().split_once(':')?;
            let dev =
                smithay::reexports::rustix::fs::makedev(major.parse().ok()?, minor.parse().ok()?);
            DrmNode::from_dev_id(dev).ok()
        })
        .collect::<Vec<_>>();
    nodes.sort_by_key(DrmNode::dev_id);
    nodes.dedup();
    assert!(
        nodes.len() >= 2,
        "transfer-cache fixture requires two validated DRM sysfs node identities; no GPU device is opened"
    );
    (nodes[0], nodes[1])
}

pub fn context() -> ContextId<Image> {
    ContextId::new()
}
