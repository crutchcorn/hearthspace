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

## Focus-independent X11 selection export

The [selection-control patch](../../patches/satellite-selection-control.patch)
uses the standard `ext-data-control-v1` protocol to export X11 owners without
requiring a previous keyboard-focus serial. Hearthspace exposes Smithay's
manager only to its private bridge. Satellite does not consume data-control
host offers; host reads retain the ordinary data-device/primary-selection path.
An instance-specific random MIME marker suppresses self-export loops; it grants
no authority. Weak source references preserve each selection generation.

The patch also validates live X ownership before applying queued clear/destroy
events, retires the matching export, and permits a fresh host claim when the
selection is genuinely unowned. This prevents stale clears and obsolete X
timestamps from breaking primary selection. Existing notices, pin and package
version remain unchanged. This is a local repair, not an upstream backport;
see the ledger for exercised boundaries and removal requirements.

## Window metadata fallback

The [identity patch](../../patches/satellite-window-identity.patch), applied after
selection-control, preserves XRes-credential process identity when WM_CLASS is
absent or blank, follows subsequent class/instance changes, and handles property
deletion. It is a local repair, not an upstream backport. Upstream pin, package
version and notices remain unchanged; regression/removal details are in the ledger.

## Focus and maximization state

The [window-state patch](../../patches/satellite-window-state.patch), after
window-identity, reads target-window focus hints on keyboard enter and retains
the last focused toplevel for globally-active clients. It propagates host and
EWMH maximization, preserving unrelated state, and decodes full XDG state words.
The XConnection hook is defaulted. This local repair changes no pin, package
version or notices; coverage and removal conditions are in the patch ledger.

## Reproducible build identity and distribution

The [build-identity patch](../../patches/satellite-build-identity.patch), after
window-state, removes parent-repository Git discovery and unused build-time
dependencies. The library reports its pinned package/local-base identity; the
Hearthspace helper reports the complete upstream revision. Pin, package and MPL
notices remain unchanged. Installed releases include this complete modified
source tree in `share/doc/hearthspace/xwayland-satellite-source.tar.xz`, together
with the MPL notice and provenance. See the patch ledger for refresh/removal.

## Explicit popup parent

The [popup-parent patch](../../patches/satellite-popup-parent.patch), after
build-identity, prefers a mapped WM_TRANSIENT_FOR with a live XDG role over the
last hover/focus heuristic. Unknown, unmapped, stale and self parents cannot be
selected. Popup role classification is unchanged. This is a local repair, not
an upstream backport; pin, package and notices remain unchanged. Regression
and removal requirements are in the patch ledger.

## Desktop-file XDND bridge

The [desktop-DND patch](../../patches/satellite-desktop-dnd.patch), after
popup-parent, translates host URI-list Copy offers into XDND 2–5. It reuses the
selection/INCR transfer path, makes foreign reads fallible, caps DND chunks and
matches each INCR requestor/property. Fresh source windows isolate generations;
status/drop/finish retain target and offer ownership. SCTK retains undropped
Wayland offers until leave; dropped offers finish only on matching X success.
This is a local adaptation informed by the standard XDND specification and
upstream issue 133, not a merged backport or a general bidirectional bridge.
Pin, package and MPL notices remain unchanged. Scope, regressions and removal
requirements are in the patch ledger.

## Terminal size increments

The [size-increment patch](../../patches/satellite-size-increments.patch), after
desktop-DND, preserves ICCCM base/increment hints in ordinary X buffer resizing.
It bounds-checks property fields, rounds within client bounds and skips grid
rounding for maximized/fullscreen states. Host XDG geometry remains responsible
for canvas anchoring. This is an independent local repair, not an upstream
backport; pin, package and notices are unchanged. See the ledger for regressions
and removal conditions.

## XDND proxy destinations

The [DND-proxy patch](../../patches/satellite-dnd-proxy.patch), after
size-increments, preserves self-verified XDND proxies from the direct XWM path.
Delivery uses the proxy, while messages and replies retain the actual target;
proxy destruction revokes the offer. This local adaptation follows pinned
Smithay's validation and the XDND specification, not a merged Satellite backport.
Pin, package and notices remain unchanged. See the ledger for scope, real-client
regressions and removal conditions.

## Window geometry and floating restore

The [window-geometry patch](../../patches/satellite-window-geometry.patch), after
DND-proxy, remembers floating buffer dimensions for unspecified restore axes and
translates validated GTK frame extents to committed XDG visible geometry. Buffer
sizing includes those insets; host size hints exclude them. Deletion and invalid
metadata clear the crop, and Satellite caption bounds remain represented.
This is a local adaptation of XDG semantics and the pinned Smithay X11 mapping,
not an upstream backport. Pin, package and notices remain unchanged. Regression
boundaries and removal requirements are in the ledger.
