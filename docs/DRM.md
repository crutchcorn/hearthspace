# DRM/KMS backend

Hearthspace can run inside another desktop or own a Linux seat and its displays.
The compositor keeps the same window, canvas, and input policies in either mode;
the backend supplies device access, rendering targets, and presentation timing.
This guide is for contributors changing that boundary.

## Backend modes

| Mode | Display and input ownership | Typical use |
| --- | --- | --- |
| `winit` | A host Wayland/X11 desktop supplies a window and input events | Nested development and interaction checks |
| `headless` | An offscreen renderer supplies the output | Deterministic integration tests and capture |
| `udev` / DRM/KMS | Hearthspace acquires seat devices and presents to KMS | Native desktop sessions |

Explicit compositor flags select the backend:

```text
--headless -> headless
--winit    -> winit
--tty      -> udev/DRM/KMS
auto       -> winit when WAYLAND_DISPLAY or DISPLAY is set, otherwise udev
```

The selected backend must be enabled in the build. Nested and native support use
`winit` and `udev` features respectively. See [development](DEVELOPMENT.md) for
build and run commands, and [native testing](NATIVE_TESTING.md) for VT ownership,
recovery, and acceptance procedures.

## Compositor and backend ownership

The compositor core owns Wayland protocols, window identity and stacking, hit
testing, focus, the canvas viewport, shell requests, and scene construction.
A drag or resize should use those shared policies regardless of the display
backend.

The backend owns how input enters the event loop, how render buffers are
acquired, how they reach a display, and which OS resources must pause or resume.
Native display ownership includes hotplug, modes, page flips, and device access;
it is more than replacing a nested window's framebuffer.

| Concern | Nested winit | Native DRM/KMS |
| --- | --- | --- |
| Outputs | Host manages physical displays | Hearthspace manages connector/CRTC targets |
| Rendering target | Host window surface | Leased scene buffers and presenter-owned scanout buffers |
| Input | Host-translated events | libinput through the active seat session |
| Presentation pacing | Host/window events and buffer submission | Pending frames and DRM completion events |
| Cursor | Host cursor or composited client surface | Smithay cursor plane or cached-scene composition |
| Capture | Renderer readback | Exact submitted plane composition associated with presentation |

## Native display concepts

DRM/KMS is the kernel API for configuring displays and presenting buffers.
A connector represents a display connection, such as HDMI or DisplayPort. A CRTC
is a scanout engine driving a mode on a connector. A mode describes resolution
and refresh timing. Planes carry scanout layers; the primary plane holds the main
framebuffer and a cursor plane can carry a separate pointer image.

GBM allocates renderable scanout buffers. EGL creates the graphics context, GLES
renders the scene, and a KMS commit submits the buffer for presentation. A
page-flip completion is evidence about the submitted frame reaching scanout;
finishing GLES scene construction is not.

Render ownership and scanout ownership remain separate roles even when they use
the same DRM node. The renderer owns imports and texture resources; scanout owns
connectors, CRTCs, output surfaces, and pending flips. Preserve that distinction
when extending device support.

## Outputs and scaling

The native backend opens the seat's DRM devices and keeps their scanout resources
separate. Each target has its own leased scene pool, scene damage history, and
presenter-owned `DrmCompositor` with scanout buffers and pending frame state. The compositor publishes outputs backed by successfully created surfaces
and renders the desktop rectangle belonging to each one. Discovery alone does
not establish that a connected display can present.

One GPU owns the GLES scene renderer, application imports, and effect caches.
Outputs attached to that GPU render directly into leased, cursor-free scene
buffers. For an output on another GPU, Hearthspace renders into a persistent
scene texture.
Compatible explicit format/modifier pairs use Smithay's multi-GPU renderer,
including when the sharing format differs from the target's scanout format.

When direct sharing is unavailable, Smithay copies the rendered image through
CPU memory and uploads it to the target GPU. Both routes copy complete output
images into target-local scene buffers. The presenter imports the completed
image into its own GLES context and composites it into scanout. Unchanged scenes
remain cached for cursor updates. The source intermediate's format need
not match the target's scanout format. Sharing and copying remain automatic
backend decisions, separate from an application's
[graphics preference](APPS.md#application-graphics-preference).

DRM connector and CRTC handles are local to a device. Completion events therefore
carry device identity and incarnation as well as the CRTC. Native output IDs
combine the connector with its physical GPU path, so discovery order cannot
give one GPU another GPU's saved preferences. Settings shows a short display
name; commands use the complete ID reported by `status`. Changing the primary
display does not change which GPU renders the scene.

For controlled startup diagnostics, `HEARTHSPACE_DRM_RENDER_DEVICE` prefers one
scene GPU by its exact physical identity under `/sys/devices/`. Use the raw
identity obtained by resolving `/sys/class/drm/<enumerated-card>/device` with
`readlink -f`, where `<enumerated-card>` is the card name from the seat's device
discovery log. Use that exact result, not a card number or an encoded output
name. The backend matches it against the seat's enumerated GPUs and still opens
the other devices for their outputs. An unset variable preserves automatic
selection; an invalid or ambiguous identity rejects startup before DRM devices
are acquired.

The preference survives an initially inactive seat and is consumed when the
first scene renderer is established. If the validated GPU disappears or fails
to initialize, existing fallback remains available and the selection log records
the requested identity, actual renderer, and reason. Subsequent recovery follows
the normal device policy; a returning preferred GPU does not replace a healthy
renderer. This startup diagnostic is separate from application launch preferences.

Output records own client-visible mode, scale, and logical placement. KMS targets
own physical scanout dimensions. The output view is logical, and rendering applies
its integer scale to reach the framebuffer. Pointer bounds, per-output shell
surfaces, lock surfaces, and captures must be reconciled when geometry changes.
See [size units](SIZE_UNITS.md).

Saved preferences can outlive a disconnected connector. Live outputs and saved
monitor rows therefore have separate lifetimes; hotplug must not erase user
preferences or manufacture a currently connected output from a saved row.
Older bare connector preferences apply only when the known output identities
identify one owner. Ambiguous preferences remain stored and unapplied; saving
the display arrangement in Settings records the qualified IDs.

## Session and device access

Native mode uses Smithay's libseat session integration for DRM and input-device
access. Device use follows the active seat session. It must not bypass that
ownership by treating device nodes as ordinary files available at all times.

On session pause, Hearthspace stops native commits and direct input, resets active
input sequences, and invalidates capture and lock-presentation evidence. Wayland
clients remain connected. An inactive VT is an expected session state, not a
request to retire every application window.

Activation rebuilds or reactivates devices and output surfaces, reconciles output
state, and requests a redraw. System-sleep recovery also has an explicit path:
a wake signal alone is not proof that libseat paused and resumed the devices.
Recovery uses bounded retries and requires presentation on the expected outputs
before declaring success. Driver calls are synchronous, so a retry budget cannot
guarantee a hard upper bound on a blocked driver call.

Device recovery retains healthy GPUs while retrying failed devices. Removing the
scene GPU selects an available renderer and invalidates scene textures, effect
caches, and dmabuf feedback. Allocation feedback updates existing surface
subscriptions as well as the default. Existing protocol bindings accept newly
advertised formats, while previously advertised allocations still reach the
renderer for validation. Existing client buffers must import on the new
renderer; device failover cannot guarantee compatibility for allocations a
client made on the removed GPU.

Locking retains an opaque fallback while outputs are unavailable or being rebuilt.
A fresh lock presentation must satisfy the current output barrier; a stale page
flip cannot acknowledge it. See [session locking](SESSION_LOCK.md).

## Event loops and pacing

The main calloop loop owns Wayland dispatch, libseat, udev discovery, libinput,
application state, imports, and scene rendering. One presenter thread per DRM
device owns that device's DRM notifier, KMS surfaces, GLES context, and Smithay
`DrmCompositor` instances. KMS completion stays on that thread, including
`frame_submitted`, which can itself perform submission work.

For each output, main publishes a cursor-free scene with a producer fence and an
`Arc<Slot<GbmBuffer>>` lease. A DMA-BUF clone alone does not reserve a pool slot.
The presenter adopts completed scenes without waiting on an unfinished scene
before moving the cursor over the previous image. Producer and consumer fences
keep leases unavailable until both users finish. A full pool leaves scene work
dirty and resumes on a capacity notification; it does not wait for KMS on main.

Scene and cursor updates have separate revisions. With one flip pending per
output, the presenter retains the newest desired state and submits it after that
output completes. Cursor plane movement can reuse primary contents. Software
cursor fallback also uses the cached scene, preserving the same ownership and
presentation boundary.

Completions carry device incarnation, output generation, submission sequence,
scene sequence, cursor identity, and security epoch. The worker rejects old DRM
timestamps; one untimed modeset completion can retire a buffer and request a
verified follow-up, but cannot prove capture or lock presentation. Main rejects
stale or reordered notifications. A cursor-only completion cannot acknowledge a
newer scene's capture or release its pending scene state.

Scene completions service that output's application callbacks independently of
other outputs. A window tree and its popups select one cadence by projected
overlap with primary-output ties. Cursor callbacks use their image identity;
callback-only opportunities are paced separately and do not claim presentation.

Reconfiguration, pause, and removal revoke admission before asynchronous worker
cleanup. The main record retains libseat ownership until the worker and its
resources retire. A stuck worker prevents replacement for that device rather
than accumulating threads and pools. Final session teardown can join workers;
ordinary input and hotplug callbacks do not. Lock changes revoke old security
epochs and require a current opaque shield or lock scene before acknowledgement.

## Dmabuf feedback

GPU clients need device and format information for compatible allocation.
Hearthspace advertises renderer-importable dmabuf formats and uses the native
renderer device as the feedback main device. Imported buffers are then sampled
by that renderer.

Import failures are client/rendering compatibility failures, distinct from KMS
mode-setting failures. Useful diagnostics include dimensions, format, planes,
modifiers, and the selected device. An advertised format is not a promise that
every allocation from every driver will import. See
[rendering](RENDERING.md#client-buffers-and-readiness) for readiness and synchronization.

## Damage tracking and KMS damage

Renderer damage describes regions that must be redrawn in a framebuffer. KMS
damage clips describe changed regions passed to the kernel for presentation.
They serve different consumers and can be enabled independently.

Hearthspace retains renderer-side damage tracking but submits native frames
without KMS damage clips. Each new `DrmCompositor` disables clip generation
through the [local Smithay policy patch](../patches/README.md). This preserves the
existing NVIDIA correctness policy while keeping renderer damage incremental. A future KMS-damage optimization needs device-appropriate
validation and a fallback when the driver rejects it; it must not disable ordinary
renderer damage as a side effect.

Cross-GPU composition keeps source-texture history separate from the target's
rotating GBM buffers. The persistent source has age one after a successful render
and age zero when invalidated. The initial transfer path copies the full output,
so it does not depend on the destination buffer's age. Rendering, image transfer,
and KMS submission preserve their completion fences in that order.

Each output owns its Smithay transfer intermediate and the last target-GPU
completion fence. Before rewriting the persistent scene or intermediate, the
source renderer waits for that completion. Drawing, readback, or KMS submission
failure does not discard a known fence. If rendering cannot supply trustworthy
completion, the intermediate is discarded instead of reused. Resizing, renderer
replacement, and seat pause release the affected output's scene, intermediate,
and completion together. One output's dimensions cannot evict another output's
transfer storage.

## Failure and diagnostics

Initialization can fail when required seat, graphics, or output resources cannot
be created. During a running session, client import failures, hotplug changes, or
individual submissions can be recoverable. Keep clients connected where possible,
retain dirty work, and log the affected device or output. Remaining alive does not
by itself prove that presentation recovered.

Native captures use renderer readback and presentation acknowledgement. Lock,
output reconfiguration, device removal, and inactive-session transitions invalidate
pending capture state. A screenshot should not be reported as a newly presented
frame on the basis of stale pixels alone.

Use [native testing](NATIVE_TESTING.md) for safety exits and failure triage,
[cursor handling](CURSOR.md) for native cursor fallback, and
[tested environments](TESTED_ENVIRONMENTS.md) for recorded environments. A nested
or headless pass does not establish native device compatibility.

## Virtual display resizing

Virtual displays can change their preferred DRM mode when the host window
resizes. Hearthspace follows the normal udev connector-change path, selects the
updated mode, and replaces its scene and scanout buffers. A connected monitor's
Wayland output stays advertised during that replacement; its size updates when
the replacement surface is ready. Actual unplug still removes the output.
This preserves shell controls and client output identity through a resize.

A pending modeset uses Smithay's synchronous `commit_frame`, which generates no
page-flip event. Hearthspace then forces an ordinary flip, even for unchanged
pixels. Only that verified completion can acknowledge capture, locking, or
client presentation. This avoids treating a virtual GPU's modeset timestamp
from the preceding vblank as proof of a newly presented frame.

The producer and presenter share clones of one GBM device for each seat-owned
DRM file description. Their EGL contexts and renderers remain thread-owned.
Creating independent GBM devices over cloned descriptors gives them independent
buffer bookkeeping for the same kernel GEM-handle namespace. The
[kernel's GEM-close contract](https://docs.kernel.org/gpu/drm-uapi.html#drm-ioctl-gem-close)
requires userspace to close a duplicated handle only once. Sharing the GBM
owner prevents one importer from retiring a handle another still uses.

The initial Apple virgl full-repaint workaround was removed after correcting
this ownership boundary and validating incremental repainting. See
[tested environments](TESTED_ENVIRONMENTS.md) for the observed configurations
and retained evidence. No vendor-specific compositor protocol is required for
the observed Parallels preferred-mode updates.
