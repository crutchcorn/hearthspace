# Wallpapers

Settings → Background offers **Canvas dots**, **Photo**, and **GLSL**. Photo
wallpapers keep their existing PNG fit controls. Choosing GLSL opens a file picker
for `.glsl` or `.frag` files; cancelling keeps the current wallpaper. Selection is
saved by settingsd and restored on the next session.

The desktop renders Canvas dots using a bundled fragment shader too. It retains
the 28-unit canvas grid and the light/dark theme colors. Panning and zooming move
this grid with the canvas; output density remains separate from canvas zoom.

## A first shader

Save this as `gradient.glsl`, choose GLSL in Settings, and select the file:

```glsl
precision highp float;
varying vec2 v_coords;

void main() {
    gl_FragColor = vec4(v_coords.x, v_coords.y, 0.25, 1.0);
}
```

Hearthspace uses Smithay's GLES pixel-shader facility. Write **GLSL ES 1.00** and
omit `#version`: the renderer supplies it. The entry point is `void main()`, with
color written to `gl_FragColor`. Declare the uniforms you use. Wallpapers are
opaque procedural backgrounds; alpha is forced to one. Discarded fragments reveal the desktop's black clear color. Image textures and
Shadertoy-specific inputs are not supplied.

This interface follows the canvas-space approach described by
[DriftWM's shader guide](https://malbiruk.github.io/driftwm/shaders.html#your-first-shader).
Hearthspace's bundled dots implementation preserves its own grid and palette;
it does not depend on DriftWM or its wallpaper files.

## Coordinates and uniforms

| Input | Type | Meaning |
| --- | --- | --- |
| `v_coords` | `varying vec2` | Coordinates from zero to one across this output, top to bottom. |
| `size` | `uniform vec2` | Output extent in canvas units: logical output size divided by canvas zoom. |
| `u_camera` | `uniform vec2` | Canvas coordinate at this output's top-left, including its desktop position. |
| `u_zoom` | `uniform float` | Canvas zoom; one is the normal view. |
| `u_scale` | `uniform float` | Output pixel density, independent of canvas zoom. |
| `u_time` | `uniform float` | Seconds since compositor startup, sampled at approximately 30 Hz. |
| `u_background_color` | `uniform vec3` | Theme background color as normalized RGB. |
| `u_dot_color` | `uniform vec3` | Theme grid color as normalized RGB. |
| `u_grid_origin` | `uniform vec2` | `u_camera` reduced modulo 28 on the CPU, preserving the built-in grid's phase on distant canvas locations. |

For a pattern anchored to the canvas, use:

```glsl
precision highp float;
varying vec2 v_coords;
uniform vec2 size;
uniform vec2 u_camera;
uniform vec3 u_background_color;
uniform vec3 u_dot_color;

void main() {
    vec2 canvas = v_coords * size + u_camera;
    vec2 cell = mod(canvas, 40.0);
    float line = 1.0 - step(1.0, min(cell.x, cell.y));
    gl_FragColor = vec4(mix(u_background_color, u_dot_color, line), 1.0);
}
```

GLSL floats have limited precision. Patterns using a large absolute `u_camera`
may lose fine detail far from the canvas origin. The built-in dots use the
reduced `u_grid_origin` instead.

## Animation and reload

A shader that declares and uses `u_time` schedules wallpaper redraws at about
30 Hz. For example:

```glsl
precision mediump float;
varying vec2 v_coords;
uniform float u_time;

void main() {
    float pulse = 0.5 + 0.5 * sin(u_time + v_coords.x * 6.28318);
    gl_FragColor = vec4(0.08, 0.15 + 0.15 * pulse, 0.22, 1.0);
}
```

Static shaders and Canvas dots do not schedule animation. Reduced motion fixes
`u_time` at zero and disables the wallpaper timer; enabling motion resumes the
compositor clock. Wallpaper animation
pauses while the session is locked, the native backend is inactive, or every
output is covered by fullscreen content. A visible canvas preview can still
require the background. Animation redraws the desktop scene; its cost depends on
shader complexity, output resolution, and GPU/transfer behavior. This cadence is
a scheduling target, not a physical frame-rate guarantee.

After editing a file, choose **Reload GLSL file**. There is no automatic source-file
watcher. Settings reports compilation or file errors and the desktop falls back
to Canvas dots, retaining the chosen path so it can be repaired and reloaded.
Sources must be regular UTF-8 files no larger than 1 MiB. Changing to another
wallpaper or choosing **Use default background** clears the custom selection.

The Settings thumbnail labels GLSL; it does not run the shader as a preview.
Custom GLSL runs only in the unlocked desktop scene. The lock client uses its
theme background rather than executing the desktop shader.
