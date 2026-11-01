# Vendored Smithay

Source: [Smithay at `5c2ba4132556a08abc70ae1771d275ec2cfc031f`](https://github.com/Smithay/smithay/tree/5c2ba4132556a08abc70ae1771d275ec2cfc031f),
package version 0.7.0, MIT. The upstream notice is preserved in `LICENSE.txt`.

This directory contains upstream `src/`, `build.rs`, the two GBM C feature probes,
README, license, and original manifest. Runtime source changes are the
[XSettings flush patch](../../patches/smithay-xsettings-flush.patch): after
queuing the settings property, flush the X11 connection so idle clients receive
the change. Initialization, set, remove, and clear share that publication path.
It adds no X11 round trip or public API. The
[move/resize cancellation patch](../../patches/smithay-xwm-moveresize-cancel.patch)
adds a default XwmHandler callback and forwards EWMH cancellation (direction 11)
to the compositor; existing handlers retain their behavior. The
[touch cancellation patch](../../patches/smithay-touch-cancel.patch) cancels all
active and pending-up targets even after their last events were framed, retains
per-client cancellation deduplication, and retires cancelled slot/frame state.
It changes only `src/input/touch/mod.rs`, without a public API change.
The [dmabuf feedback format patch](../../patches/smithay-dmabuf-feedback-formats.patch)
shares creation validation across globals and buffer params and monotonically
extends it before publishing new default feedback. Earlier advertised formats
still reach the compositor's import handler; new legacy bindings receive only
current main-tranche formats. It changes `src/wayland/dmabuf/mod.rs` and
`src/wayland/dmabuf/dispatch.rs` without adding public APIs. Hearthspace keeps
surface feedback current through the existing `SurfaceDmabufFeedbackState` API.
The independent [multi-GPU transfer cache patch](../../patches/smithay-multigpu-transfer-cache.patch)
adds an opaque caller-owned `TransferCache` and a `MultiRenderer` builder that
selects its borrowed intermediate slot. Device/context identity binds that slot;
existing size/Fourcc validation, allocation, negotiation, direct sharing, and
CPU copying are unchanged. Existing callers retain the manager's pair cache.
Caller-owned caches require their own completion synchronization and explicit
invalidation. The upstream basis is the pinned source above; the latest inspected
multigpu file commit, [`347b2b30225b0425f0b44dc4c590e768f7dbe73e`](https://github.com/Smithay/smithay/blob/347b2b30225b0425f0b44dc4c590e768f7dbe73e/src/backend/renderer/multigpu/mod.rs),
still retains one intermediate per device pair.

`Cargo.toml.orig` is the exact upstream manifest. The standalone `Cargo.toml`
omits upstream workspace members, examples, benchmarks, development-only
dependencies, and the custom release profile; target autodiscovery is disabled.
Runtime/build dependencies, features, and package identity remain unchanged.
Hearthspace's dependency stays pinned to the original Git revision, with the
workspace Cargo patch selecting this copy.

To refresh:

1. Copy these same files from a compatible upstream revision and retain its
   original manifest and notices.
2. Normalize the standalone manifest as described above, preserving all runtime
   dependencies and features.
3. Reapply source patches only if still needed. Follow the verification and
   removal criteria in the [patch ledger](../../patches/README.md).
4. Refresh `Cargo.lock` and run the idle-X11, X11 interaction, and
   `smithay_touch_cancel` and `compositor::udev::gpu::feedback::tests` protocol
   regressions plus the live GTK3 appearance and
   native touch cancellation checks, plus the headless `touch_cancel::` and
   `power_confirmation::` regressions, before removing a patch or changing the
   pinned revision. Touch regressions live in
   `crates/hearthspace-compositor/tests/smithay_touch_cancel.rs` and its dedicated
   support module; they use the workspace's existing Wayland client dependency.
5. For the transfer cache patch, a separate run of the compositor's
   `smithay_transfer_cache` integration target with `--features udev` and test
   flags `--ignored --test-threads=1` is required on a host with two validated
   DRM sysfs node identities. These cases are ignored by default so software-only
   workspace/CI runs remain usable. Record the explicit run's executed/passed/
   failed counts separately from the default ignored count. The fixture opens
   no GPU devices and retains strict prerequisite failure when explicitly run.
   Then repeat native mixed-size cross-GPU output validation and measurement.
   The integration target uses the real manager/render/finish path with
   substitutes at the GPU-device boundary; it does not establish GPU compatibility
   or performance. See the [test guidance](../../docs/E2E_TESTING.md#smithay-transfer-cache-regressions)
   for the command and the ledger for the removal condition.

## DRM compositor damage-clip policy

The independent [damage-clip policy patch](../../patches/smithay-drm-damage-clips.patch)
adds `DrmCompositor::set_damage_clips_enabled(bool)` to the pinned upstream
compositor. The default remains enabled in both constructors. Disabling prevents
both rendered-primary and direct-scanout damage blobs, clears clips from current,
pending, queued, and prepared frames, and forces the next successful submission
to clear the KMS property even when the scene is unchanged. It preserves renderer
damage tracking, buffer ages, frame resources, and completion synchronization.
The setting survives buffer/state resets and `clear()`; a newly constructed
compositor starts enabled and its caller must opt out again.

This local extension changes only `src/backend/drm/compositor/mod.rs` and adds
`src/backend/drm/compositor/damage.rs`. No manifest, upstream notice, existing
repair, or atomic surface code changes. The existing atomic request builder
writes `FB_DAMAGE_CLIPS = 0` for a plane without clips. Reapply this independent
patch during a refresh until compatible upstream code supplies the same opt-out
and retained-frame behavior. The [patch ledger](../../patches/README.md#smithay-drm-compositor-damage-clip-policy)
defines the required native property-inspection regression boundary and removal
condition. No device-free behavioral fixture is supplied: the real compositor
requires a DRM surface, and vendor tests are outside workspace test execution.
## Applied surface state and subsurface transactions

The independent [applied-state patch](../../patches/smithay-applied-surface-state.patch)
repairs attachment-independent scale/transform/damage, synchronized descendant
commit collection, parent-owned position/stacking application, and retention of
unrecognized non-null buffers. It changes
`src/backend/renderer/utils/wayland.rs` and
`src/wayland/compositor/{cache.rs,handlers.rs,transaction.rs,tree.rs}`,
adding the private `src/wayland/compositor/tree/parent_state.rs` helper.
The upstream revision, package version, manifests, notices, and other repairs
remain unchanged; no public API is added.

[Smithay PR 2141](https://github.com/Smithay/smithay/pull/2141), open when inspected
on 2026-11-01, informed parent-owned position handling, cache override, and
applying transaction state before callbacks. This patch is a local adaptation,
not a merged upstream backport. Attachment-independent renderer updates,
avoiding implicit child commits, staged stacking with role lifetime validation,
and preserving failed non-null attachments are independent local fixes.
Layout is staged until transaction application rather than applied during
cached-state merging. Resolved pending stacking orders are copied into parent
transactions; destroying a member removes only that member from pending and
cached orders, preserving surviving moves without affecting a recreated role.

When refreshing, reapply this patch while needed and run the real App/Wayland
cursor snapshot regressions with surfaceless EGL, using the command and coverage
limits in the [patch ledger](../../patches/README.md#smithay-applied-surface-state-and-subsurface-transactions).
The failure fixture substitutes only buffer storage recognition; no native DRM
or driver-fault validation is implied. Remove this repair only after compatible
upstream code passes the independent cases without it. Preserve the other
patches until their separate removal conditions are satisfied.
