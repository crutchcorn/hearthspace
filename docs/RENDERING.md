# Rendering and client buffers

Hearthspace composes application buffers, native shell surfaces, and compositor
chrome into one desktop. This guide explains the boundaries that matter when
changing that path: buffer readiness, displayed geometry, damage, and presentation.
For output ownership and frame pacing, see [DRM/KMS](DRM.md).

## Client buffers and readiness

Wayland clients supply their own pixels. Hearthspace supports shared-memory
(`wl_shm`) buffers and GPU buffers through `zwp_linux_dmabuf_v1`.

Shared-memory buffers contain pixels the client has prepared for the compositor.
They still require renderer import or upload; shared memory does not make drawing
free. A dmabuf shares a buffer with the renderer and can carry outstanding GPU
work through implicit synchronization. Its availability for import and its
readiness for sampling are separate concerns.

The Wayland handlers queue dmabuf import requests. The event loop imports them
through the active backend's renderer and reports success or failure to the
client. This keeps renderer ownership in the backend while the protocol handlers
retain responsibility for the request. Advertised formats and device feedback
must describe what that renderer can import.

Native seat pause or temporary renderer unavailability leaves those requests
queued. The backend acquires the canonical scene renderer before consuming a
batch, so recovery does not turn a valid pending request into a client protocol
error. An unavailable secondary GPU does not block imports through a healthy
scene renderer. Actual buffer import failures still follow the protocol's
failure rules.

Before applying a surface commit, Hearthspace asks Smithay for a dmabuf readiness
blocker. When a wait is needed, a calloop source watches the buffer's readiness
and releases the blocked transaction. An already-ready buffer needs no blocker.
This avoids deliberately blocking the event loop while waiting for a client to
finish drawing. Failure to register the readiness source is logged and must be
treated as a synchronization problem when investigating corrupted output.

This path uses implicit synchronization. Import support alone does not establish
support for every format, modifier, synchronization protocol, or driver pairing.
Keep compatibility failures distinct from native mode-setting failures.

## One displayed geometry for drawing and input

A window has a saved canvas home and a displayed position. Search, switching,
frame attachment, and focused view can change its presentation without replacing
that home or its Wayland surface.

The compositor resolves a window's presentation into one projection: surface
origin, visible bounds, scale, opacity, and any preview pose. Rendering and inverse
input transforms use that projection. Animation reads use the sampled animation
time, so an input event targets the visible window rather than a different point
in its transition.

Keep output scale separate from canvas zoom. The canvas transform places content
in desktop logical coordinates; the output transform maps that view into its
framebuffer. Shell surfaces attached to the frame stay in screen space. See
[size units](SIZE_UNITS.md) for the conversion rules and
[window management](WINDOW_MANAGEMENT.md) for presentation states.

## Damage and presentation

Smithay's damage tracker identifies which parts of a render target need updating.
Buffer age describes how much history that particular target needs. Importing a
dmabuf can change the current EGL surface; Hearthspace forces a full redraw after
imports instead of trusting a back-buffer-age query for the wrong surface.

Native outputs have separate scanout buffers and damage trackers. Each output
renders its logical desktop rectangle at its own scale. A pending page flip holds
additional work as dirty state until that output can submit again. Renderer damage
remains enabled, while KMS damage clips are omitted from native submissions.
These are separate optimizations, as [the backend guide](DRM.md#damage-tracking-and-kms-damage)
explains.

The final composition also returns a GPU completion fence. Native submission
passes that fence to Smithay, which supplies a KMS input fence when supported or
waits for rendering before submitting. Finishing CPU-side scene construction
does not mean the GPU has finished writing the buffer. This dependency is
required even when KMS damage clips are disabled, and is separate from the
client-buffer readiness checks above.

When scanout belongs to another GPU, the scene texture and target buffer belong
to different renderer contexts. Smithay transfers the completed scene through
its multi-GPU drawing API and returns the target GPU's completion fence for KMS.
The source texture has its own damage history; target swapchain age must not be
used for that persistent texture. See [multi-GPU output ownership](DRM.md#outputs-and-scaling)
for the transfer and fallback boundaries.

Compositor effects also own caches. Glass backdrops cache the content beneath a
surface, including a sampling margin; titlebar foregrounds retain their rasterized
text and icons across geometry-only resizes. Cache keys and damage must include
changes to geometry, output scale, appearance, and the sampled content. See
[UI boundaries](UI.md) for material and decoration ownership.

## Locking and capture

A locked session uses a separate scene with an opaque black clear and eligible
lock surfaces. Ordinary applications, shell panels, and their glass backdrops are
excluded. A missing or transparent lock surface therefore cannot reveal the
desktop. Lock acknowledgement waits for the required output presentation barrier;
constructing a frame is not proof that it reached the display.

Native capture reads rendered output pixels and can add the hardware cursor,
which is otherwise outside the framebuffer. A requested cursor-free remote frame
uses the corresponding cursor-free scene. Capture requests are invalidated during
lock and session loss. See [session locking](SESSION_LOCK.md) and
[native testing](NATIVE_TESTING.md) for the security and capture contracts.

## Investigating slow frames

A client readiness wait, buffer import, scene render, readback, and presentation
submit can each take time. Measure them separately before assigning a cause.
Asynchronous readiness handling does not guarantee that driver calls elsewhere
in the render loop are nonblocking.

Software renderers can make import, upload, and submission expensive. Hardware
acceleration can also stall, depending on the driver, synchronization, format, and
workload. Switching from nested to native presentation does not by itself prove
that either rendering or client import is accelerated.

When reporting a rendering issue, record the backend, renderer and driver,
output scale, client buffer type, and operation that stalls. Compare a
shared-memory client with a dmabuf client where useful, and separate first-frame
cost from sustained frame pacing. Keep timing evidence with the investigation;
only measured conclusions should become documented performance expectations.

At TRACE level, `hearthspace_compositor::compositor::frame_callbacks` emits
`native frame callback batch` with the completed output, elapsed microseconds,
lock state, and number of `wl_callback.done` events sent. Empty batches are
recorded too. Compare these with native queue and completion events to separate
client callback starvation from slow rendering. A sent callback is not proof
that a client has processed it.

Include a movement interval without screenshots or screen recording when
investigating flashing. GPU readback can wait for outstanding rendering and hide
a missing synchronization dependency. A clean compositor capture does not by
itself rule out corruption during physical scanout.

A renderer-thread change requires an explicit ownership design: the GLES renderer
is thread-bound, and render elements refer to renderer resources and live Wayland
state. Preserve those lifetimes and the event loop's synchronization model when
evaluating such a change. A slow frame alone is not evidence for a particular
threading architecture.
