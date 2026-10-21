# UI Boundaries

Hearthspace should treat the compositor as the owner of composition, window management, canvas transforms, and input routing. It does not need to become a general-purpose application UI framework.

The compositor should draw the UI that is tightly coupled to those responsibilities:

- Client windows, by composing app-provided Wayland buffers.
- Window decorations, including title bars, resize handles, shadows, and focus state.
- Primitive shell chrome, such as debug overlays, selection rectangles, drag indicators, and simple global controls.
- Compositor effects, including pan, zoom, clipping, stacking, and viewport animations.

Richer interface surfaces can be ordinary Wayland clients. The compositor can still give those clients special placement and behavior, but they should render themselves into Wayland buffers like any other app.

## Coordinate Spaces

Hearthspace UI should be described in two coordinate spaces.

Screen-space UI is fixed to the visible output or nested compositor window:

- Frame-attached shell controls, groups, pinned windows and compact desktop items.
- Debug overlay.
- Command palette.
- Lock screen.
- Notifications.
- Cursor and drag indicators.

Canvas-space UI lives on the infinite canvas and moves/zooms with the viewport:

- App windows.
- Window decorations.
- Sticky notes.
- Window groups.
- Labels and annotations.
- Spatial launchers.
- Canvas-local inspectors or tools.

Canvas-space UI should use the same viewport transform as application windows. In practice, these can all be modeled as canvas items:

```text
CanvasItem
- ClientWindow
- WindowDecoration
- NativeCanvasWidget
- ShellSurface
- GroupFrame
- Annotation
```

## UI Frameworks

An external UI framework is most useful when rendering rich shell clients, not when rendering the compositor's internal chrome.

The preferred model is:

```text
UI framework app -> Wayland buffer -> Hearthspace composites it
```

For example, a Xilem-based launcher, settings app, inspector, command palette, or note editor could run as a Wayland client. Hearthspace can recognize it as shell UI and place it in screen-space or canvas-space depending on the surface's role.

Embedding a UI framework directly into the compositor render loop is a different tradeoff. It requires the framework to render into a texture, image, or scene that the compositor can consume directly, while the compositor keeps ownership of the event loop, renderer, Wayland protocols, and input routing. Xilem's app driver expects to own the winit application/window/event-loop flow, so the full framework is not a good fit for direct embedding — but its widget layer, Masonry, can rasterize into a buffer the compositor consumes directly, which Hearthspace already does for window title-text (`compositor/masonry_titlebar.rs`).

## Recommended Boundary

Keep these responsibilities in the compositor core:

- Wayland protocol handling.
- Window placement and stacking.
- Canvas viewport offset and scale.
- Screen-space versus canvas-space placement policy.
- Window decorations and primitive shell chrome.
- Decoration policy, defaulting normal windows to client-side chrome and enabling compositor chrome only for server-side decoration negotiation.
- Input hit testing and forwarding.
- Compositor-owned animation state.

Prefer shell clients for richer UI:

- Launcher.
- Settings.
- Inspector.
- Command palette.
- Rich canvas widgets.
- Notes, cards, and panels.
- Complex forms, lists, and text editing.

This keeps the compositor small and predictable while still allowing rich UI to exist inside Hearthspace.

The [frame controls and their dropdowns](SHELL.md) split their glass appearance across this
boundary. The compositor blurs the wallpaper and application windows behind each
surface; the shell draws a translucent tint, border, icons, and text over it. Both
use the shared corner radius of 20 logical pixels. Movable controls sample the real stack below their owner. A group has one glass
backing, with its member controls kept sharp above it.

Each visible shell surface caches a GPU texture covering its bounds and the blur's sampling
margin. Smithay damage tracking refreshes it when the desktop beneath it changes;
unchanged regions reuse their textures. The two blur passes stay on the GPU and
require no CPU pixel readback. If the shader is unavailable, the shell keeps its
translucent tint and controls, with blur disabled for that renderer.

Server-decorated titlebars and the full Settings window opt into a compositor
backdrop. It samples the wallpaper and windows below its position in the stack,
excluding the owner's controls, shadow, and windows above it. Overlapping glass
windows resolve from back to front, so each sees the completed glass below.
The client and titlebar paint their translucent warm tints over this backdrop,
and the compositor keeps the rounded edge sharp at the output scale.

Each glass region is cached separately for each output. Its
40-logical-pixel Gaussian runs on smaller GPU textures, then resolves to the
visible output size; unchanged content reuses the cached result. The sampling
margin accounts for damage outside the window, and output-edge clamping avoids
dark borders. This path performs no CPU pixel readback and does not run in the
lock screen or overview. Fullscreen windows use the normal fullscreen path.

Server decorations use the reference's 44-pixel header, app icon, centered grip,
and 32-pixel maximize/restore and close controls. Masonry rasterizes their text
and vectors into cached strips at the highest intersecting output density.
Resizing repositions and clips those strips without rasterizing them again;
only changed text, icon, control state, color or pixel density invalidates them.
A shared thread-local font context retains font discovery and loaded font bytes
across redraws. The GPU paints the growing header material, divider and grip,
so even height-only resize avoids a full-width CPU paint.

The compositor clips the complete decorated window to a 19-pixel inner radius.
A separate GPU frame paints its one-pixel outer border (20-pixel outer radius),
inset highlights and soft shadows.

The active orange frame encloses every normal application, including clients
with their own decorations. Client-decorated content and controls retain their
own shape; its compositor border follows the advertised rectangular window
geometry. Frames belong to each window's stack position, beneath its popups and
any higher windows. Fullscreen content, shell panels and overview thumbnails do
not receive these frames. Cached shader elements track geometry, focus and
hover without allocating a window-sized decoration texture.

## Shared visual system

The native orange/light UI follows `hearthspace-ui-experiments` revision
`91a2f88`. `hearthspace-ui-components` owns warm surfaces, ink, orange accents,
control states, and the reference SVG path geometry. The paths render as native
Masonry vectors in shell clients, Settings, and server-side window decorations.
The default canvas background is a seamless muted dot field.

Shell glass uses an 82% warm tint and stronger muted/accent ink to retain
contrast over dark application content. Settings uses the reference's lighter
ink, Liberation Sans text metrics, a 54% window tint (76% over photos), and the
compositor backdrop described above.

Settings controls use `settings_button` and `settings_surface` from
`hearthspace-ui-components`. These decorate native Masonry controls with inset
highlights, soft shadows, a radial accent wash, and keyboard-only focus rings.
Set sizing and padding on the native button before wrapping it; put dynamic
`.disabled(...)` on the returned material view so the decoration and input state
stay in sync. The Settings Materials story exposes selected and disabled states.
Shell buttons continue to use property classes; local background or border
colors override those selectors.

Below 560 logical pixels of client width, Settings replaces its 160-pixel sidebar
with a 49-pixel header. The current category button opens the navigation over
the page; selecting a category or pressing Escape closes it and returns focus
to the button. Tab follows header, navigation, then page controls, without a
focus trap. Resizing preserves page widgets and unfinished input. The window's
minimum client size is 400 by 420 pixels.

Masonry layers do not expose a backdrop filter for content inside a client
window. Settings choice menus and the AI confirmation therefore use a precomposed
opaque raised backing with the same tint and highlights. This avoids sharp
text showing through the popup; it approximates the browser's blurred backing.
The compact category menu keeps the reference's translucent raised tint over
the page, without an additional opaque backing.
The AI modal dims the client window without blurring the underlying controls.
The Settings window itself uses actual GPU backdrop blur.

Settings enables light/dark mode and the reference's six accent colors. A shared
`Palette` carries explicit colors; `hearthspace_properties(palette, style)` builds
native defaults. Windows cache these defaults in an `Arc` and replace it only
when appearance changes, preserving widget IDs, input focus, popup state and
unfinished text. Custom painters read a normal Masonry `Palette` property and
invalidate paint when it changes. There is no mutable global theme. Compatibility
constructors still produce the original orange/light defaults for stories.

Dark glass keeps the reference surface tints and transparency. Glass defaults
use `Palette::for_glass()` to adjust shell ink and slightly reduce dark hover
opacity so nested launcher controls remain readable over bright windows.
Settings and window decorations retain the reference ink and hover tokens.
Compositor frames update shader uniforms
and titlebar foreground caches on appearance changes. Geometry-only resizing
continues to reuse the foreground bitmaps; palette resolution is cached too.
Changing modes regenerates the default dot field while retaining decoded user
wallpapers. See [Settings](SETTINGS.md#linux-appearance-publication) for application
preferences published through the desktop portal and XSettings.

[Desktop icons](DESKTOP_ITEMS.md) use transparent Xilem shell surfaces in canvas
coordinates, with compositor-owned placement and input. They reuse the launcher
catalog and native application artwork. [Canvas search](CANVAS_SEARCH.md) reuses
these live surfaces and the normal window renderer with temporary compositor
transforms. Its native field and control previews use the reference's glass
panel material. The [layout editor](SHELL.md#change-layout) uses the same boundary:
Xilem renders independent controls, groups, tooltips and confirmation dialogs;
the compositor owns placement, input capture, guides, clipping and transitions.
The shared frame model lives in `hearthspace-common::frame_layout`.

Toolbar turns compose their live native surfaces, outlines, shadows and resolved
glass into temporary GPU textures. The compositor rotates that composition around
the grabbed point while the native controls reflow to their final orientation.
Placement, guide and reveal animations respect reduced motion. Confirmation
backdrops use a separate 8-pixel blur beneath the native dialog. No client-buffer
readback is needed for these effects.

The prototype's simulated apps and drawing tools remain separate work.

## When To Use A UI Framework

Manual compositor rendering is enough for simple, tightly-coupled chrome:

- Title bars.
- Resize handles.
- Shadows.
- Basic buttons.
- Simple overlays.
- Selection rectangles.
- Debug labels.
- Viewport animations.

Reach for an external UI system when the interface needs substantial application UI features:

- Text editing.
- Menus.
- Scrollable or virtualized lists.
- Rich layout.
- Accessibility.
- Theming.
- Complex interaction state.
- Reusable widgets.
- Forms and settings screens.

The default should be to implement those as Wayland shell clients first. Direct compositor embedding should only be reconsidered if primitive compositor rendering becomes a real limitation for compositor-owned chrome.

## Placement Examples

```text
Normal app window
- Canvas-space
- User movable

Shell corner controls
- Screen-space
- Fixed at the four output corners

Canvas note/editor
- Canvas-space
- Moves and zooms with the canvas

Command palette
- Screen-space
- Floats above everything
```
