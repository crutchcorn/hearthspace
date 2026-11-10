use super::super::event_cutoff::{SurfaceEvent, SurfaceEventCutoff};
use super::{CapturedFrame, CursorUpdate, OutputConfig, cursor::CursorCache, retirement::Scene};
use smithay::{
    backend::{
        allocator::{
            Fourcc,
            gbm::{GbmAllocator, GbmBufferFlags, GbmDevice},
        },
        drm::{
            DrmDevice, DrmDeviceFd, DrmEventMetadata,
            compositor::DrmCompositor,
            exporter::gbm::{GbmFramebufferExporter, NodeFilter},
        },
        renderer::{
            Bind,
            element::Id,
            gles::{GlesRenderer, GlesTexture},
            utils::{CommitCounter, DamageBag},
        },
    },
    output::OutputModeSource,
    utils::{Buffer, Size, Transform},
};
use std::time::{Duration, Instant};

pub(super) type Compositor = DrmCompositor<
    GbmAllocator<DrmDeviceFd>,
    GbmFramebufferExporter<DrmDeviceFd>,
    Submission,
    DrmDeviceFd,
>;

pub(super) struct Submission {
    pub sequence: u64,
    pub scene_sequence: u64,
    pub cursor_sequence: u64,
    pub cursor_image: Option<(Id, u64)>,
    pub security_epoch: u64,
    pub lock_generation: Option<u64>,
    pub scene_changed: bool,
    pub capture: Option<Result<CapturedFrame, String>>,
    pub diagnostics: SubmissionDiagnostics,
}

pub(super) struct SubmissionDiagnostics {
    pub cursor_plane: bool,
    pub primary_scanout: bool,
    pub window_id: Option<u64>,
    pub primary: Option<(u32, CommitCounter)>,
    pub primary_reused: bool,
    pub render_us: u64,
    pub queue_started: Instant,
}

pub(super) enum RenderOutcome {
    Primed,
    Queued,
    Empty,
    Revoked,
}

/// Only these external compositor operations are substituted in scheduler tests.
pub(super) trait SubmissionOps {
    fn reset_state(&mut self) -> Result<(), String>;
    fn render_and_queue(&mut self) -> Result<RenderOutcome, String>;
}

pub(super) enum Completion<T> {
    Ignored,
    Untimed,
    Presented(T),
}

/// Production output scheduling, shared by turn, vblank, and lifecycle paths.
pub(super) struct Schedule {
    pub pending: bool,
    pub dirty: bool,
    pub failed: bool,
    pub opportunity_at: Option<Instant>,
    force_commit: bool,
    cutoff: SurfaceEventCutoff,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            pending: false,
            dirty: true,
            failed: false,
            opportunity_at: None,
            force_commit: false,
            cutoff: SurfaceEventCutoff::now(),
        }
    }
}

impl Schedule {
    pub fn invalidate_security(&mut self) {
        self.dirty = true;
        self.opportunity_at = None;
        // Pixel equality cannot satisfy a new security generation. Smithay's
        // reset_state forces a nonempty submission until queue_frame succeeds.
        self.force_commit = true;
    }

    pub fn submit(
        &mut self,
        now: Instant,
        refresh: Duration,
        ops: &mut impl SubmissionOps,
    ) -> Result<(), String> {
        if self.pending || self.failed || !self.dirty {
            return Ok(());
        }
        if self.force_commit {
            ops.reset_state()?;
        }
        match ops.render_and_queue()? {
            RenderOutcome::Primed => {
                // A synchronous modeset is not timestamped presentation evidence.
                // Force an event-producing flip and run again without new input.
                self.invalidate_security();
                self.opportunity_at = Some(now);
            }
            RenderOutcome::Queued => {
                self.pending = true;
                self.dirty = false;
                self.force_commit = false;
                self.opportunity_at = None;
            }
            RenderOutcome::Empty if self.force_commit => {
                return Err("security barrier produced no physical submission".into());
            }
            RenderOutcome::Empty => {
                self.dirty = false;
                self.opportunity_at.get_or_insert(now + refresh);
            }
            RenderOutcome::Revoked => {}
        }
        Ok(())
    }

    pub fn complete<T>(
        &mut self,
        metadata: Option<&DrmEventMetadata>,
        submitted: impl FnOnce() -> Result<Option<T>, String>,
    ) -> Result<Completion<T>, String> {
        if !self.pending {
            return Ok(Completion::Ignored);
        }
        let classification = self.cutoff.classify(metadata);
        if classification == SurfaceEvent::Stale {
            return Ok(Completion::Ignored);
        }
        let Some(submission) = submitted()? else {
            return Ok(Completion::Ignored);
        };
        self.pending = false;
        if classification == SurfaceEvent::Untimed {
            self.invalidate_security();
            return Ok(Completion::Untimed);
        }
        Ok(Completion::Presented(submission))
    }
}

pub(super) struct Output {
    pub config: OutputConfig,
    pub compositor: Compositor,
    pub schedule: Schedule,
    pub cursor_caps: Size<u32, Buffer>,
    pub cursor: Option<CursorUpdate>,
    pub cursor_cache: CursorCache,
    pub scene: Option<Scene>,
    pub desired: Option<Scene>,
    pub scene_id: Id,
    pub scene_damage: DamageBag<i32, Buffer>,
    pub last_presented_scene: Option<u64>,
    pub last_primary: Option<(u32, CommitCounter)>,
    pub next_submission: u64,
    pub refresh: Duration,
    pub capture_buffer: Option<GlesTexture>,
}

impl Output {
    pub fn new(
        drm: &mut DrmDevice,
        gbm: &GbmDevice<DrmDeviceFd>,
        renderer: &GlesRenderer,
        config: OutputConfig,
    ) -> Result<Self, String> {
        let schedule = Schedule::default();
        let surface = drm
            .create_surface(config.key.crtc, config.mode, &[config.connector])
            .map_err(|error| error.to_string())?;
        let formats =
            <GlesRenderer as Bind<smithay::backend::allocator::dmabuf::Dmabuf>>::supported_formats(
                renderer,
            )
            .ok_or("presenter renderer has no framebuffer formats")?;
        let cursor_caps = drm.cursor_size();
        let mut compositor = Compositor::new(
            OutputModeSource::Static {
                size: config.size,
                scale: 1.0.into(),
                transform: Transform::Normal,
            },
            surface,
            None,
            GbmAllocator::new(
                gbm.clone(),
                GbmBufferFlags::SCANOUT | GbmBufferFlags::RENDERING,
            ),
            // Only admitted fullscreen elements expose Wayland storage. Imported
            // DMA-BUFs need not carry a node hint; Smithay/KMS must test them.
            GbmFramebufferExporter::new(gbm.clone(), NodeFilter::All),
            [Fourcc::Argb8888, Fourcc::Xrgb8888],
            formats,
            cursor_caps,
            Some(gbm.clone()),
        )
        .map_err(|error| error.to_string())?;
        compositor.set_damage_clips_enabled(false);
        let hz = config.mode.vrefresh().clamp(1, 1000);
        tracing::debug!(output = config.name, crtc = ?config.key.crtc, "configured device presenter output");
        Ok(Self {
            config,
            compositor,
            schedule,
            cursor_caps,
            cursor: None,
            cursor_cache: CursorCache::default(),
            scene: None,
            desired: None,
            scene_id: Id::new(),
            scene_damage: DamageBag::new(4),
            last_presented_scene: None,
            last_primary: None,
            next_submission: 0,
            refresh: Duration::from_nanos(1_000_000_000 / u64::from(hz)),
            capture_buffer: None,
        })
    }
}
