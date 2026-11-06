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

## Embedded child stdio ownership

The independent [embedded-stdio patch](../../patches/satellite-embedded-stdio.patch)
adds defaulted RunData hooks for Xwayland stdin/stdout. Defaults retain upstream
inheritance. Hearthspace's embedded entry point supplies null descriptors so its
private Wayland and readiness channels cannot be inherited by nested Xwayland.
No source version, license or CLI default changes. See the
[ledger](../../patches/README.md#satellite-embedded-child-stdio) for the regression
and removal condition. This local API extension is not an upstream backport.

## Explicit Xwayland executable

The [managed-executable patch](../../patches/satellite-managed-xwayland.patch)
adds a defaulted `RunData::xwayland_binary` method, retaining upstream's PATH
lookup for other consumers. Hearthspace validates its sibling
`hearthspace-Xwayland` and receipt, then supplies that absolute path and
`-surface-input-targets`. It never substitutes a system server. This is a local
API extension, not an upstream backport; pin, package and notices are unchanged.
See the patch ledger for verification and removal requirements.

## Host appearance and legacy resources

The [host-settings patch](../../patches/satellite-host-settings.patch) adds an
optional, private `xwayland_satellite_settings_v1` global. Satellite receives
the host's theme before readiness and publishes `Net/ThemeName` alongside its
existing DPI entries, retaining later scale and theme updates. Absent the
extension, upstream behavior remains available. A resource update also avoids
creating RESOURCE_MANAGER over legacy fallback files (or unknown HOME).
Hearthspace's helper seeds its shared cursor size before publishing DISPLAY.
This is a local adaptation, not an upstream backport; version, pin and notices
remain unchanged. Verification/removal are in the patch ledger.
