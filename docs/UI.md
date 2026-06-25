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

- Top toolbar.
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

Shell toolbar
- Screen-space
- Fixed above the canvas

Canvas note/editor
- Canvas-space
- Moves and zooms with the canvas

Command palette
- Screen-space
- Floats above everything
```
