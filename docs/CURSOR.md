# Mouse Cursor

This document explains Hearthspace's cursor model: what a hardware cursor is,
what a software cursor is, why we prefer one over the other, and how cursor art
is represented in the project.

The policy is:

- Use a hardware cursor by default when Hearthspace owns native display output.
- Use a software cursor only as a fallback or for cases where the cursor must be
  part of the composited scene.
- Store cursor art as standard Xcursor assets generated from `.cursor` source
  files, not as Rust-written pixel buffers.

## The Mental Model

The cursor has three separate concerns:

- Pointer position: where input is logically located.
- Cursor shape: what visual icon should represent that pointer right now.
- Cursor presentation: how that icon gets onto the screen.

Pointer position is input state. Cursor shape is compositor policy. Cursor
presentation is backend/display policy.

Keep those concerns separate. Input handling should decide that the pointer is
over a resize edge, title bar, client area, or canvas. The backend should decide
whether that desired shape is presented through a hardware cursor, a host window
cursor, or a software-rendered cursor.

## Hardware Cursor

A hardware cursor is not drawn into the compositor's main framebuffer. Instead,
the display system scans out a small cursor buffer separately from the normal
scene. On DRM/KMS this is usually exposed as a cursor plane or legacy cursor API
attached to a CRTC.

Hardware cursors are the preferred native path because they are:

- Lower latency: pointer movement can update cursor position without waiting for
  the whole scene to redraw.
- Cheaper: moving the cursor does not require recompositing windows,
  backgrounds, and shell UI.
- Cleaner for damage tracking: the cursor does not dirty the framebuffer behind
  it.
- More robust under load: if the compositor misses a frame, the cursor can still
  track input independently of scene rendering.

Hardware cursors also have constraints:

- Cursor buffers are small and have driver-reported size limits.
- The buffer format is dictated by the display API, not by the image format we
  use for art.
- The cursor position is output/CRTC-local, so global logical pointer coordinates
  must be converted before presentation.
- The hotspot matters. The visual image is positioned so the hotspot lands on the
  pointer coordinate, not so the top-left pixel lands there.
- Multi-output setups need handoff logic when the pointer crosses from one CRTC
  to another.
- Drivers can reject cursor buffers or movement operations, so the compositor
  must have a fallback story.

In Hearthspace's native backend, hardware cursor should be the first attempted
presentation mode. If hardware cursor setup or operation is unavailable, we fall
back to software presentation rather than failing the compositor.

## Software Cursor

A software cursor is drawn like any other compositor render element. It becomes
part of the framebuffer for that frame.

Software cursors are useful because they work anywhere the compositor can render:

- They do not depend on DRM cursor-plane support.
- They can be used in headless or unusual test paths.
- They can represent cursors that need to participate in compositor effects,
  screenshots, transforms, or other scene-owned behavior.

They are not the default native path because every pointer movement now has to be
treated as a scene change:

- The compositor must request a redraw when the pointer moves.
- Damage tracking has to account for the old and new cursor locations.
- Cursor motion can appear delayed if rendering is delayed.
- The cursor can leave stale trails if buffer age and damage are handled
  incorrectly.

Use software cursor presentation as a fallback, not as the normal native path.
When software presentation is active, it should still consume the same desired
cursor shape as the hardware path. The input model should not care which
presentation path is active.

## Nested Backends

When Hearthspace runs nested inside another desktop through the winit backend,
the host window system already has its own cursor machinery. In that mode,
Hearthspace asks the host window to show the desired cursor icon instead of
drawing or scanning out the cursor itself.

Conceptually this is still the same split:

- Hearthspace owns pointer hit testing and desired cursor shape.
- The host compositor/window system owns cursor presentation.

Do not add software cursor drawing to the nested path unless there is a specific
reason the host cursor cannot represent the desired behavior.

## Cursor Shape Policy

Hearthspace's compositor-owned cursor shapes are intentionally small and tied to
window-management interactions:

- Default pointer for canvas, normal client areas, and inactive chrome.
- Vertical resize for north/south resize edges.
- Horizontal resize for east/west resize edges.
- Diagonal resize for resize corners.

The desired cursor shape should be derived from semantic compositor state, not
from backend details. Examples:

- Hit testing says the pointer is over a resize border, so the desired shape is a
  resize cursor.
- A compositor-managed resize is in progress, so the desired shape remains the
  resize cursor for the active edges.
- The pointer returns to normal canvas/client space, so the desired shape returns
  to default.

Client-provided Wayland cursor images should eventually feed into the same
conceptual pipeline: they may affect desired cursor shape or image while the
pointer is focused on that client, but they should not bypass the backend
presentation policy.

## Coordinates And Hotspots

The pointer coordinate is the logical input location. The cursor image has a
hotspot: the pixel inside the image that should line up with that pointer
coordinate.

For a software cursor, presentation position is:

```text
render_position = logical_pointer_position - cursor_hotspot
```

For a native hardware cursor, presentation position is the same idea after first
converting the logical pointer position into the target output or CRTC's local
coordinate space:

```text
crtc_position = output_local_pointer_position - cursor_hotspot
```

This is why cursor assets must carry hotspot metadata. If the hotspot is wrong,
resize edges, clicks, and the visible cursor tip will feel misaligned even though
input routing is correct.

## Cursor Assets

Cursor art should be maintained as assets, not as code.

Hearthspace uses Xcursor files because they are a standard cursor container that
stores:

- One or more image sizes.
- Hotspot coordinates per image.
- Pixel data.
- Optional animation timing.

Generated cursor assets live in `assets/cursors/`. Editable sources live in
`assets/cursors/source/`.

The `.cursor` files in `assets/cursors/source/` are `xcursorgen` configuration
files. A line like this:

```text
24 12 12 ns-resize.png 0
```

means:

- `24`: cursor image size.
- `12`: X hotspot.
- `12`: Y hotspot.
- `ns-resize.png`: source image.
- `0`: animation delay.

The generated files in `assets/cursors/` are what the compositor embeds and
parses at runtime. The source SVG/PNG/config files are what contributors edit.

Do not hard-code cursor pixels in Rust. If the art changes, update the source
asset and regenerate the Xcursor file.

## Asset Regeneration

Cursor asset tooling is documented in `CONTRIBUTING.md`. The required tools are
ImageMagick and `xcursorgen`:

```sh
sudo apt-get install -y x11-apps imagemagick
```

Regenerate resize cursor assets from the workspace root:

```sh
magick assets/cursors/source/ns-resize.svg assets/cursors/source/ns-resize.png
xcursorgen assets/cursors/source/ns-resize.cursor assets/cursors/ns-resize
```

## Contributor Rules

Use these rules when changing cursor behavior:

- Keep pointer position, desired cursor shape, and cursor presentation separate.
- Prefer hardware cursor presentation in the native backend.
- Keep software cursor presentation available as a fallback.
- Make hardware and software presentation consume the same desired cursor state.
- Preserve hotspot metadata through the asset pipeline.
- Do not generate cursor art from Rust code.
- Do not make input handlers directly own backend cursor presentation.
- Treat hardware cursor failure as a recoverable presentation failure, not as a
  compositor startup failure.
