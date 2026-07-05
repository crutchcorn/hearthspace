# Size Units

Hearthspace uses several size and coordinate spaces at once. The important rule is to name the space before doing math. Values with the same number type are not interchangeable.

## Spaces

### Physical Output Pixels

Physical output pixels are hardware or backend pixels.

Use them for:

- DRM/KMS modes and scanout buffers.
- GBM surfaces and damage trackers tied to a framebuffer.
- Native output descriptors, such as a panel mode of `2560x1600`.

In code, these are commonly `smithay::utils::Physical` sizes or raw mode sizes. Example: `OutputRecord::size` stores the physical mode size.

### Hearthspace Layout Coordinates

Hearthspace layout coordinates describe output placement and the compositor's screen-space map. Today these intentionally use physical-sized output extents even though Smithay's type parameter is `Logical` in several places.

Use them for:

- Monitor arrangement positions from settings.
- Output rectangles used for pointer clamping and native output render views.
- Screen-space placement of mirrored shell chrome across outputs.

Example: a `2560x1600` output at `4x` still occupies `2560x1600` in Hearthspace layout coordinates. A second monitor to the right begins at `x = 2560`, not `x = 640`.

This keeps pointer movement, monitor arrangement, and KMS output views aligned with physical desktop geometry.

### Client Logical Coordinates

Client logical coordinates are the sizes and positions Wayland clients see before their output scale is applied.

Use them for:

- `xdg_toplevel` configure sizes and bounds.
- Shell clients, settings clients, and any other toolkit-rendered surface size requests.
- Avoiding GPU texture allocations that exceed device limits.

Formula:

```text
client_logical = ceil(physical_pixels / output_scale)
```

Example: a `2560` physical-pixel-wide output at `4x` is `640` client logical pixels wide. If Hearthspace configures a shell client to `2560` logical pixels instead, the client may allocate a `10240` pixel-wide buffer and fail.

### Client Buffer Pixels

Client buffer pixels are the actual pixels submitted by a Wayland client.

Formula:

```text
buffer_pixels = client_logical * output_scale
```

Use them when reasoning about:

- Client-submitted `wl_buffer` sizes.
- WGPU/Vello texture limits in shell clients.
- Whether a configured logical surface size is safe for high-scale outputs.

The compositor usually receives these through Wayland surfaces rather than calculating them directly.

### Canvas Coordinates

Canvas coordinates are Hearthspace's infinite workspace coordinates for normal app windows and canvas-space compositor chrome.

Use them for:

- Normal window positions.
- Window dragging and resizing.
- Viewport pan and zoom.
- Compositor-owned canvas-space decorations.

Canvas coordinates are transformed to screen/layout coordinates by `viewport_offset` and `viewport_scale`.

### Render Coordinates

Render coordinates are the coordinates used by a specific backend render pass.

Use them for:

- Renderer element locations.
- Output-local rendering after subtracting the output view origin.
- Damage tracking and framebuffer submission.

For native multi-output rendering, each output view has a Hearthspace layout location and physical-sized extent. Rendering relocates elements relative to that output's framebuffer.

## Conversion Guide

```text
physical output size + output scale -> client logical size
client logical size * output scale -> client buffer pixels
canvas point + viewport transform -> Hearthspace layout/screen point
Hearthspace layout point - output location -> output-local render point
```

Prefer explicit helper functions at conversion boundaries. Do not rely on type names alone, because current Hearthspace layout coordinates may be typed as Smithay `Logical` while still representing physical-sized desktop extents.

## Current Project Conventions

- `MonitorConfig.width` and `MonitorConfig.height` are physical output sizes.
- `MonitorConfig.x` and `MonitorConfig.y` are Hearthspace layout coordinates.
- `MonitorConfig.scale` is the Wayland integer output scale to advertise.
- `monitor_logical_width` and `monitor_logical_height` currently return physical-sized layout extents, not client logical dimensions.
- `OutputRecord::logical_size` currently returns physical-sized layout extents.
- `OutputRecord::client_logical_size` is for Wayland client configure sizing.
- Shell bar configure sizes must use client logical dimensions, not Hearthspace layout dimensions.
- Native output render views use Hearthspace layout rectangles so monitor arrangement maps directly to real output positions.

## Common Mistakes

- Do not pass physical-sized layout widths to `xdg_toplevel` configure when output scale is greater than `1`.
- Do not divide monitor arrangement positions by output scale unless the feature explicitly operates in client logical coordinates.
- Do not use client buffer pixels for pointer clamping or monitor arrangement.
- Do not assume every `Logical` typed value means client logical size in Hearthspace code.

## Example

Given an internal display at `2560x1600` with output scale `4`:

```text
physical output size:        2560 x 1600
Hearthspace layout extent:   2560 x 1600
client logical size:          640 x 400
client buffer size:          2560 x 1600
```

A shell bar mirrored across that output should be configured to `640x48` client logical units. The client will submit a buffer around `2560x192` pixels at `4x`, which stays within the output and GPU texture limits.
