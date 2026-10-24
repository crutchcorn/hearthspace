# Mouse cursor

Hearthspace separates where the pointer is, what it looks like, and how it reaches
the display. Input routing owns pointer position and the desired cursor state;
the backend owns presentation. Changing the presentation path must not change
hit testing or the destination of a click.

## Presentation by backend

| Backend | Named compositor cursor | Client-provided cursor surface |
| --- | --- | --- |
| Native DRM/KMS | Hardware cursor when available; software fallback | Composited in software |
| Nested winit | Host window cursor | Host cursor hidden; surface composited in software |
| Headless | Composited in software | Composited in software |

A client can also request a hidden cursor. Visibility and image changes flow
through the same desired cursor state; they do not give the client control over
the display backend.

## Hardware cursor

A hardware cursor is scanned out separately from the main scene. On the native
backend, Hearthspace uses DRM cursor operations to assign a small buffer to a
CRTC and move it. This can update the pointer image's position without
recompositing the desktop, reducing scene damage and work during pointer motion.
It still depends on input processing and driver behavior; it is not a guarantee
of responsiveness when the event loop stalls.

The hardware path must respect the driver's buffer dimensions and pixel format.
It selects the output under the pointer, converts logical coordinates to that
output's local pixels, and clears the previous CRTC's cursor when crossing outputs.
Output density also affects the image size and hotspot.

If hardware cursor creation fails, Hearthspace keeps software presentation
available. A failed cursor assignment or movement clears the active hardware
cursor and falls back to software for that update. Cursor failure should remain a
recoverable presentation failure rather than a reason to stop the desktop.

## Software cursor and client images

A software cursor is a render element in the compositor's framebuffer. It needs a
redraw when it moves, and the renderer must account for the old and new locations.
Its motion therefore follows frame presentation. Headless rendering always uses
this path because it has no separate host or KMS cursor.

Focused Wayland clients can supply cursor surfaces through
`wl_pointer.set_cursor`. Hearthspace renders the surface tree using its committed
hotspot and hides the hardware or host cursor to avoid drawing two cursors. This
also supports client artwork that is outside the compositor's packaged shape set.
A destroyed client cursor surface falls back to the named shape.

In nested mode, named shapes use the host window system's cursor. Client surfaces
are the exception: they are composed into Hearthspace's window. Keep that
exception explicit when investigating cursor appearance or capture differences.

## Shape policy

Window-management hit testing selects the desired shape. Resize borders and active
resize operations use horizontal, vertical, or diagonal resize cursors. Other
interactions can request semantic shapes such as grab or crosshair.

The embedded native asset set contains the default pointer and four resize
shapes: `ns-resize`, `ew-resize`, `nwse-resize`, and `nesw-resize`. The native and
headless asset lookup falls back to the default pointer for other named shapes.
The host cursor path can represent a broader set. Smithay named-cursor events are
also mapped to the supported default/resize set.

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

The software path places the cursor in logical scene coordinates and lets output
rendering apply density. A client cursor uses the surface's hotspot and buffer
mapping. Keep canvas zoom out of cursor density: the pointer is screen-space UI.
See [size units](SIZE_UNITS.md) for the surrounding coordinate conventions.

Native capture can add the hardware cursor to the captured frame, since the main
framebuffer alone does not contain it. Software cursors are already part of the
scene. Capture code must distinguish these paths so the cursor appears once.

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
magick assets/cursors/source/ns-resize.svg assets/cursors/source/ns-resize.png
(
    cd assets/cursors/source
    xcursorgen ns-resize.cursor ../ns-resize
)
```

Running `xcursorgen` from the source directory lets it resolve the PNG named in
the configuration. Repeat the same steps for the other resize shapes when
changing the set. See [dependencies](DEPENDENCIES.md) for tool setup.
