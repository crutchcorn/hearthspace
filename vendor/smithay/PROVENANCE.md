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
