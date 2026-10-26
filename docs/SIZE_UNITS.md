# Size units

A display mode, a window configure, and a canvas position can all contain the
number `2560` while describing different things. Name the coordinate space before
doing math, and apply each scale once.

## Physical output pixels

Physical pixels describe a backend's framebuffer. DRM/KMS modes, GBM allocations,
and scanout damage use this space. A `2560 × 1600` output mode has that many
framebuffer pixels regardless of the scale advertised to clients.

`MonitorConfig.width` and `height`, and `OutputRecord::size`, store physical mode
dimensions. Physical panel dimensions in millimeters are separate metadata.

## Desktop logical coordinates

Output placement, pointer bounds, screen-space shell placement, and output render
views use logical coordinates. An output's logical extent is its physical mode
size divided by its integer output scale, rounded up:

```text
logical_extent = ceil(physical_extent / output_scale)
```

A `2560 × 1600` output at `4×` occupies `640 × 400` logical units. A monitor placed
immediately to its right starts at `x = 640` when the first monitor starts at zero.
The Settings arrangement preview uses those same logical extents.

`MonitorConfig.x` and `y` are logical placement coordinates; `scale` is the
advertised integer output scale. `OutputRecord::logical_size` and
`client_logical_size` agree, and the monitor layout helpers divide mode dimensions
by scale. Do not treat these as physical-sized desktop rectangles.

The active layout normalizes its origin. It also has a compatibility path for old
saved positions expressed in physical coordinates: if the direct interpretation
is disconnected, a connected, nonoverlapping converted layout may be used.
Overlapping active layouts fall back to a side-by-side arrangement. This recovery
behavior is not a reason to write new physical-coordinate placements.

## Client logical coordinates and buffer pixels

Wayland surface coordinates and `xdg_toplevel` configure sizes describe client
logical geometry. Configure a window to the logical size it should occupy;
passing the physical mode size can make a high-scale client allocate an oversized
buffer.

For a client rendering at an integer buffer scale without an additional viewport
transform:

```text
buffer_pixels = surface_logical_size × buffer_scale
```

For example, a `640 × 48` logical surface rendered at `4×` uses a `2560 × 192`
pixel buffer. This is a sizing example, not a fixed shell-panel height or a
promise that every renderer accepts that allocation.

A client's chosen buffer scale need not equal every intersecting output's scale.
Surface buffer scale, buffer transform, and viewport state determine how its
buffer maps to surface coordinates. Let Smithay resolve those protocol details;
do not infer a surface's logical geometry from raw buffer dimensions alone.

Output scale affects pixel density. Canvas zoom affects the displayed size and
position of work. Neither changes the other, and applying both as buffer-scale
multipliers would scale content twice.

## Canvas coordinates

Canvas coordinates locate ordinary windows, desktop items, and canvas-attached
controls in the workspace. The viewport offset and zoom map them to desktop
logical coordinates. Panning and zooming change the view while preserving the
item's canonical canvas home.

Frame attachment, fullscreen, focused view, and temporary search/switcher previews
have their own presentation policy. Use the compositor's resolved presentation
for rendering and inverse input transforms rather than assuming every window
uses the canvas transform directly. See [rendering](RENDERING.md) and
[window management](WINDOW_MANAGEMENT.md).

## Output-local render coordinates

Each native output renders a logical rectangle of the desktop into its physical
framebuffer. For an unrotated output, the basic conversion is:

```text
output_local_logical = desktop_logical - output_logical_origin
framebuffer_pixels = output_local_logical × output_scale
```

Keep the framebuffer extent physical and the render view logical. A renderer may
stage elements at unit scale and apply output scaling later; converting those
elements to physical pixels early would apply density twice.

Pointer hit testing uses logical positions. Hardware cursor placement converts
that position into output-local pixels and subtracts a hotspot in the same pixel
space. Nested touchscreen input similarly converts host-window physical positions
to the compositor's logical output rectangle. See [cursor handling](CURSOR.md).

## Conversion example

For a `2560 × 1600` output at `4×`, placed at logical `(640, 0)`:

| Quantity | Value |
| --- | --- |
| Physical framebuffer | `2560 × 1600` pixels |
| Desktop logical extent | `640 × 400` units |
| Logical point at output center | `(960, 200)` |
| Output-local logical point | `(320, 200)` |
| Framebuffer point | `(1280, 800)` pixels |

Round at the boundary that requires integer coordinates. Retain fractional input
motion and animation values until then so repeated small updates do not disappear.

## Rules for changes

- Use logical sizes for client configures and monitor arrangement.
- Use physical sizes for framebuffer allocation and scanout.
- Keep surface buffer scale, output density, and canvas zoom distinct.
- Use the same displayed projection for painting and input hit testing.
- Check mixed output scales and nonzero output origins when changing conversions.
