use super::bridge_policy::{Failures, Slots, poll_slots};
use crate::compositor::udev::{
    identity::DeviceToken, transfer::vulkan::Slot, vulkan_bridge::VulkanBridge,
};
use smithay::{
    backend::{
        allocator::{Fourcc, dmabuf::Dmabuf},
        drm::DrmNode,
        renderer::{ContextId, gles::GlesTexture},
    },
    utils::{Physical, Size},
};

#[derive(Clone, PartialEq)]
pub(in crate::compositor::udev) struct Endpoint {
    pub node: DrmNode,
    pub token: DeviceToken,
    pub physical: String,
    pub context: ContextId<GlesTexture>,
}

#[derive(Clone, PartialEq)]
pub(in crate::compositor::udev) struct Pair {
    pub source: Endpoint,
    pub target: Endpoint,
}

#[derive(Clone, PartialEq)]
pub(in crate::compositor::udev) struct SlotKey {
    pub pair: Pair,
    pub output: String,
    pub size: Size<i32, Physical>,
    pub format: Fourcc,
}

#[derive(Default)]
pub(in crate::compositor::udev) struct BridgeState {
    slots: Slots<SlotKey, Slot>,
    failures: Failures<Pair>,
    stopped: bool,
}

impl BridgeState {
    pub fn permits(&self, key: &SlotKey) -> bool {
        !self.stopped
            && VulkanBridge::admission_open()
            && self.failures.permits(&key.pair)
            && !self.slots.entries.iter().any(|entry| {
                entry.retired
                    && [&entry.key.pair.source, &entry.key.pair.target]
                        .into_iter()
                        .any(|old| {
                            [&key.pair.source, &key.pair.target]
                                .into_iter()
                                .any(|next| old.physical == next.physical && old != next)
                        })
            })
    }

    pub fn has_room(&self) -> bool {
        self.slots.has_room()
    }
    pub fn active(&mut self, key: &SlotKey) -> Option<&mut Slot> {
        self.slots.active(key)
    }

    pub fn insert(&mut self, key: SlotKey, value: Slot) -> bool {
        self.slots.insert(key, value).is_ok()
    }

    pub fn replace_output(&mut self, key: &SlotKey) {
        self.slots
            .retire(|old| old.output == key.output && old != key, Slot::retire);
    }

    pub fn fail(&mut self, key: &SlotKey) {
        self.failures.record(key.pair.clone());
        self.slots.retire(|old| old.pair == key.pair, Slot::retire);
        if !VulkanBridge::admission_open() {
            self.retire_all();
        }
    }

    pub fn retire_all(&mut self) {
        self.slots.retire(|_| true, Slot::retire);
    }

    pub fn stop(&mut self) {
        self.stopped = true;
        self.retire_all();
    }

    pub fn retire_node(&mut self, node: DrmNode) {
        self.slots.retire(
            |key| key.pair.source.node == node || key.pair.target.node == node,
            Slot::retire,
        );
    }

    pub fn retire_output(&mut self, output: &str) {
        self.slots.retire(|key| key.output == output, Slot::retire);
    }

    pub fn scanout_blocked(&self, dma: &Dmabuf) -> bool {
        self.slots.retired_any(|slot| {
            !slot.complete
                && slot
                    .resources
                    .as_ref()
                    .is_some_and(|resources| resources.holds.iter().any(|(_, held)| held == dma))
        })
    }

    pub fn poll(&mut self) {
        if !VulkanBridge::admission_open() {
            self.retire_all();
        }
        poll_slots(
            &mut self.slots,
            &mut self.failures,
            |key| key.pair.clone(),
            Slot::poll,
            Slot::retire,
            |slot| slot.complete,
            |key, error| {
                tracing::warn!(%error, output = key.output, "retiring failed Vulkan transfer");
            },
        );
        if !VulkanBridge::admission_open() {
            self.retire_all();
        }
    }

    pub fn needs_poll(&self) -> bool {
        self.slots
            .entries
            .iter()
            .any(|entry| entry.value.needs_poll() || (entry.retired && entry.value.complete))
    }
}
