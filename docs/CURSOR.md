# Mouse cursor

Hearthspace separates where the pointer is, what it looks like, and how it reaches
the display. Input routing owns pointer position and the desired cursor state;
the backend owns presentation. Changing the presentation path must not change
hit testing or the destination of a click.

## Presentation by backend

| Backend | Named compositor cursor | Client-provided cursor surface |
| --- | --- | --- |
| Native DRM/KMS | Smithay cursor plane when eligible; cached-scene fallback | Same plane eligibility and fallback |
| Nested winit | Host window cursor | Host cursor hidden; surface composited in software |
| Headless | Composited in software | Composited in software |

A client can also request a hidden cursor. Visibility and image changes flow
through the same desired cursor state; they do not give the client control over
the display backend.

## Native presentation

Each DRM device has a presenter thread owning its Smithay `DrmCompositor`
instances, renderer, DRM event source, and submissions. The main loop owns
libinput, Wayland protocols, hit testing, and scene construction. It publishes
cursor updates independently of cursor-free scene images.

The presenter combines the last completed scene with a separate `Kind::Cursor`
image. Smithay can assign that image to a cursor plane when its dimensions,
format, and KMS state are supported. Named shapes and client-provided artwork use
the same mechanism. A rejected plane candidate is composited over the cached
scene; this does not require another canvas render or cross-GPU scene transfer.

If queueing a selected cursor plane returns an invalid-input or KMS-test rejection,
the presenter resets buffer ages and retries once with cursor-plane assignment
disabled. The retry renders and captures its own composition. Revocation and
other errors do not enter this retry, and a rejected second attempt follows
ordinary output recovery. Only an accepted submission can complete a capture.

Cursor movement reuses the image identity and buffer, changing only its output
position. Dirty updates coalesce while an output has a pending flip. The output's
completion permits its latest state to submit independently of other outputs.
The presenter can reuse unchanged primary-plane contents, although swapchain
buffer age can require a repaint from the cached scene during warm-up.

This separates KMS scheduling from the main loop. It does not move input ingestion
to another thread: slow main-loop work can still delay newly arriving pointer
samples. Driver calls can also delay work on other outputs of the same GPU.
Hardware planes and nonblocking atomic commits alone do not guarantee smoothness.

## Client images and callbacks

Smithay authorizes `wl_pointer.set_cursor` and owns the cursor surface role.
Hearthspace snapshots applied surface-tree state on the Wayland thread, including
buffer mapping, density, hotspot, and subsurface bounds. No live Wayland surface
or main-renderer texture crosses into the presenter.

Simple committed SHM images are copied directly. Cropped, transformed, scaled,
GPU-backed, or composite cursor trees use cursor-only rendering and readback when
the effective artwork changes. Snapshots are immutable premultiplied ARGB8888,
bounded to 16 megapixels. An image exceeding the hardware plane dimensions can
still use software presentation. Motion, hotspot changes, and callback-only
commits reuse existing pixels; they do not trigger readback. Cursor-plane padding
is initialized to transparent pixels.

A null cursor image is intentionally hidden. A destroyed cursor surface returns
to the named shape. Import/readback errors are reported separately from intentional
absence. The main loop retains the prior presentation if a new snapshot cannot
be prepared.

Cursor callbacks follow the submitted image identity and revision. A paced
callback opportunity for unchanged artwork is separate from verified scanout:
it cannot acknowledge a capture or lock frame. Scene callbacks remain associated
with their output's scene completion.

Nested mode keeps using the host cursor for named shapes and composites client
surfaces into the host window. Headless rendering composites both kinds into its
offscreen framebuffer.

## Shape policy

Window-management hit testing selects the desired shape. Resize borders and active
resize operations use horizontal, vertical, or diagonal resize cursors. Other
interactions can request semantic shapes such as grab or crosshair.

The embedded native asset set contains the default pointer, `grab` and
`grabbing` hands, and four resize shapes: `ns-resize`, `ew-resize`, `nwse-resize`,
and `nesw-resize`. Pan canvas uses the open hand over an eligible canvas target
and the closed hand while dragging. Native and headless rendering share these
assets. Smithay named-cursor events map to the same supported set; other named
shapes fall back to the default pointer. The host cursor path can represent a
broader set.

This means backend-neutral intent does not imply identical artwork on every
backend. Extend the shared asset lookup when adding a native named shape; do not
put pixel construction or DRM calls into input handlers.

## Coordinates and hotspots

A hotspot is the point in a cursor image that aligns with the pointer. Subtract it
in the same coordinate space as the image placement.

For a native hardware cursor on an unrotated output:

```text
local_pixels = (pointer_logical - output_origin_logical) × output_scale
cursor_top_left_pixels = round(local_pixels) - hotspot_pixels
```

Native software fallback uses the same physical snapshot and placement as the
cursor plane. Nested and headless composition place the cursor in logical scene
coordinates and let output rendering apply density. A client cursor uses the
surface's hotspot and buffer mapping. Keep canvas zoom out of cursor density:
the pointer is screen-space UI.
See [size units](SIZE_UNITS.md) for the surrounding coordinate conventions.

Native capture reconstructs the exact submitted plane composition, so the cursor
appears once whether Smithay used a hardware plane or software composition. A
cursor-free capture reads the retained cursor-free scene. Both are associated
with the verified submission, rather than the pointer's latest position.

## Cursor assets

Generated Xcursor files live in `assets/cursors/`; editable SVG, PNG, and
`.cursor` configuration files live in `assets/cursors/source/`. The compositor
embeds the generated files and parses them at runtime.

Xcursor carries image dimensions, hotspots, pixels, and optional animation timing.
Hearthspace's packaged shape lookup selects an image near the requested size;
the container's animation support does not imply animated compositor cursors.

For example, this `xcursorgen` configuration line describes a nominal 24-pixel
cursor, a hotspot at `(12, 12)`, and its source PNG:

```text
24 12 12 ns-resize.png 0
```

Edit the source artwork and regenerate the Xcursor asset when changing a shape.
Do not replace asset files with Rust-written pixel buffers. Keep the hotspot
aligned with the tip or center used for input.

With ImageMagick and `xcursorgen` installed, regenerate one resize cursor from the
workspace root:

```sh
magick -background none assets/cursors/source/ns-resize.svg assets/cursors/source/ns-resize.png
(
    cd assets/cursors/source
    xcursorgen ns-resize.cursor ../ns-resize
)
```

Running `xcursorgen` from the source directory lets it resolve the PNG named in
the configuration. Repeat the same steps for the other shapes when changing the
set. Keep strokes explicitly opaque and the background transparent; inspect the
result on both light and dark backgrounds before regenerating the Xcursor file.
See [dependencies](DEPENDENCIES.md) for tool setup.
