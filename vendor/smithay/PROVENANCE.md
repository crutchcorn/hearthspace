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
[X11 raise cache patch](../../patches/smithay-xwm-raise-cache.patch) keeps the
last requested top root sibling separate from the managed EWMH stacking list,
invalidates it on external stack/lifecycle changes, and recognizes its latest
configure acknowledgment without suppressing geometry callbacks. It uses bounded
private state, keeps override-redirect windows out of the managed list, and adds
no round trip or public API. It does not synchronize XWM requests with queued
Wayland input. Upstream
[`f581531d`](https://github.com/Smithay/smithay/blob/f581531d479cf2e3307bafb913c5d2bb12860bb3/src/xwayland/xwm/mod.rs#L1084)
still has the managed-list cache shortcut as of 2026-10-31. The
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
6. For the raise cache patch, run the real-client foreign popup map/remap/restack
   regression linked in the ledger, check stable hover and geometry callbacks,
   and verify raised popups remain absent from `_NET_CLIENT_LIST_STACKING`.
   Run the separate batched-input regression and native pointer/touch acceptance;
   cache validity alone does not establish cross-connection event ordering.
