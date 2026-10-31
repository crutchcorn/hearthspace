use super::{
    BridgeError, CopyAcquire, CopyCompletion, LinearTarget, SourceImage, device::Device, sync,
};
use ash::vk;
use std::rc::Rc;

pub(super) struct Pending {
    device: Rc<Device>,
    source: Option<SourceImage>,
    target: Option<LinearTarget>,
    pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    semaphores: [vk::Semaphore; 3],
    submitted: bool,
    completed: bool,
}

impl Pending {
    pub fn prepare(
        device: Rc<Device>,
        source: &SourceImage,
        target: &LinearTarget,
        acquire: CopyAcquire,
    ) -> Result<Self, BridgeError> {
        let mut pending = Self {
            device,
            source: Some(source.clone()),
            target: Some(target.clone()),
            pool: vk::CommandPool::null(),
            command: vk::CommandBuffer::null(),
            fence: vk::Fence::null(),
            semaphores: [vk::Semaphore::null(); 3],
            submitted: false,
            completed: false,
        };
        let raw = &pending.device.raw;
        pending.pool = unsafe {
            raw.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(pending.device.identity.queue_family)
                    .flags(vk::CommandPoolCreateFlags::TRANSIENT),
                None,
            )
        }
        .map_err(|e| BridgeError::vk("create copy command pool", e))?;
        pending.command = unsafe {
            raw.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(pending.pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        }
        .map_err(|e| BridgeError::vk("allocate copy command buffer", e))?[0];
        pending.fence = unsafe { raw.create_fence(&vk::FenceCreateInfo::default(), None) }
            .map_err(|e| BridgeError::vk("create copy completion fence", e))?;
        for index in 0..3 {
            let mut export = vk::ExportSemaphoreCreateInfo::default()
                .handle_types(vk::ExternalSemaphoreHandleTypeFlags::SYNC_FD);
            let mut info = vk::SemaphoreCreateInfo::default();
            if index == 2 {
                info = info.push_next(&mut export);
            }
            pending.semaphores[index] = unsafe { raw.create_semaphore(&info, None) }
                .map_err(|e| BridgeError::vk("create copy semaphore", e))?;
        }
        sync::import(&pending.device, pending.semaphores[0], acquire.source_ready)?;
        sync::import(
            &pending.device,
            pending.semaphores[1],
            acquire.target_released,
        )?;
        pending.record(source, target)?;
        Ok(pending)
    }

    fn record(&self, source: &SourceImage, target: &LinearTarget) -> Result<(), BridgeError> {
        let raw = &self.device.raw;
        let family = self.device.identity.queue_family;
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        // External GLES contents are acquired in GENERAL. The image's creation
        // initialLayout=UNDEFINED is not a transition: using UNDEFINED here would
        // discard the producer's pixels. Keep GENERAL through the whole copy.
        let image_acquire = vk::ImageMemoryBarrier::default()
            .image(source.0.image)
            .subresource_range(range)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
            .dst_queue_family_index(family)
            .dst_access_mask(vk::AccessFlags::TRANSFER_READ);
        let buffer_acquire = vk::BufferMemoryBarrier::default()
            .buffer(target.0.buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
            .dst_queue_family_index(family)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        let image_release = vk::ImageMemoryBarrier::default()
            .image(source.0.image)
            .subresource_range(range)
            .old_layout(vk::ImageLayout::GENERAL)
            .new_layout(vk::ImageLayout::GENERAL)
            .src_queue_family_index(family)
            .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
            .src_access_mask(vk::AccessFlags::TRANSFER_READ);
        let buffer_release = vk::BufferMemoryBarrier::default()
            .buffer(target.0.buffer)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .src_queue_family_index(family)
            .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
            .src_access_mask(vk::AccessFlags::TRANSFER_WRITE);
        // All handles are from this device; retained imports bound the full
        // resources. layout.rs checked copy addressing and packed texel alignment.
        unsafe {
            raw.begin_command_buffer(
                self.command,
                &vk::CommandBufferBeginInfo::default()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .map_err(|e| BridgeError::vk("begin copy command", e))?;
            raw.cmd_pipeline_barrier(
                self.command,
                vk::PipelineStageFlags::TOP_OF_PIPE,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[buffer_acquire],
                &[image_acquire],
            );
            raw.cmd_copy_image_to_buffer(
                self.command,
                source.0.image,
                vk::ImageLayout::GENERAL,
                target.0.buffer,
                &[target.0.layout.region()],
            );
            raw.cmd_pipeline_barrier(
                self.command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::DependencyFlags::empty(),
                &[],
                &[buffer_release],
                &[image_release],
            );
            raw.end_command_buffer(self.command)
                .map_err(|e| BridgeError::vk("end copy command", e))?;
        }
        Ok(())
    }

    pub fn submit(&mut self) -> Result<CopyCompletion, BridgeError> {
        let waits = [self.semaphores[0], self.semaphores[1]];
        let stages = [vk::PipelineStageFlags::ALL_COMMANDS; 2];
        let commands = [self.command];
        let signals = [self.semaphores[2]];
        let info = vk::SubmitInfo::default()
            .wait_semaphores(&waits)
            .wait_dst_stage_mask(&stages)
            .command_buffers(&commands)
            .signal_semaphores(&signals);
        // Conservatively retain resources even if submission reports an error.
        // A later fence status, not the error itself, authorizes destruction.
        self.submitted = true;
        unsafe {
            self.device
                .raw
                .queue_submit(self.device.queue, &[info], self.fence)
        }
        .map_err(|e| {
            if e == vk::Result::ERROR_DEVICE_LOST {
                self.device.lost.set(true);
            }
            BridgeError::vk("submit image-to-buffer copy", e)
        })?;
        sync::export(&self.device, self.semaphores[2])
    }

    pub fn poll(&mut self) -> Result<bool, BridgeError> {
        if self.completed || !self.submitted {
            return Ok(true);
        }
        match unsafe { self.device.raw.get_fence_status(self.fence) } {
            Ok(done) => {
                self.completed = done;
                Ok(done)
            }
            Err(vk::Result::ERROR_DEVICE_LOST) => {
                // Vulkan's Lost Device section treats this fence-query result as
                // SUCCESS for resource-in-use rules, not for pixel validity.
                self.device.lost.set(true);
                self.completed = true;
                Ok(true)
            }
            Err(e) => Err(BridgeError::vk("poll copy completion", e)),
        }
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        if self.submitted && !self.completed && !matches!(self.poll(), Ok(true)) {
            // Last-resort shutdown quarantine: no blocking driver wait and no
            // destruction of in-use Vulkan objects. Retain their memory and
            // parent instance/device until process exit. MAX_PENDING bounds this
            // per bridge; production must not recreate a failed incarnation in
            // a tight loop. Normal retirement uses poll() and releases everything.
            std::mem::forget(self.source.take());
            std::mem::forget(self.target.take());
            std::mem::forget(self.device.clone());
            tracing::error!("quarantined an unresolved Vulkan copy until process exit");
            return;
        }
        unsafe {
            if self.pool != vk::CommandPool::null() {
                self.device.raw.destroy_command_pool(self.pool, None);
            }
            for semaphore in self.semaphores {
                if semaphore != vk::Semaphore::null() {
                    self.device.raw.destroy_semaphore(semaphore, None);
                }
            }
            if self.fence != vk::Fence::null() {
                self.device.raw.destroy_fence(self.fence, None);
            }
        }
    }
}
