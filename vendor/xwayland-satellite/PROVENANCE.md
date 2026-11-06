# Vendored xwayland-satellite

Upstream: https://github.com/Supreeeme/xwayland-satellite

Revision: `7bf5e0dcb0d4250860b24b04a6cb3ddeada515d0` (package 0.8.3).
The source, tests, font assets, and upstream notices are retained. The upstream
MPL-2.0 license remains in `LICENSE`.

Hearthspace builds this library through `crates/hearthspace-satellite`, whose
managed entry point accepts a private Wayland connection on stdin and publishes
readiness on stdout. The vendor's nested workspace is excluded from the outer
workspace; Cargo.lock pins its transitive dependencies for the runtime build.

Refresh from an exact upstream revision, preserve required notices, and rerun
the real X11 recipient, lifetime, popup, clipboard, settings, and interactive
regressions before changing the runtime default. Local dependency repairs and
their removal conditions belong in `patches/README.md`.

## Optional interaction cancellation

The independent [cancellation patch](../../patches/satellite-interaction-cancel.patch)
adds a private, optional Wayland interaction interface, remembers the exact
forwarded XDG toplevel/seat/serial, and forwards EWMH Cancel before the initiating
button filter. Unmapping/destruction removes the remembered operation. The XML
retains its MIT notice; other upstream notices and the MPL-2.0 source license
remain unchanged. A direct wayland-scanner dependency generates the client
binding; package version and upstream pin remain unchanged.

This is a local extension, not an upstream backport. At the pin, upstream
explicitly leaves Cancel unimplemented. It does not repair recipient routing
or Xwayland button-state reconciliation. Keep the failing real-client adoption
regressions. Refresh/removal requirements and coverage limits are recorded in
the [patch ledger](../../patches/README.md#satellite-optional-interaction-cancellation).
