# UI boundaries and visual language

Hearthspace keeps the work visible while its controls stay within reach. Native
shell surfaces provide menus, fields, lists, and status; the compositor places
those surfaces, routes input, and draws the effects that depend on the whole
desktop. This guide explains that division and the visual rules for extending it.
See [desktop design](DESIGN.md) for the canvas-and-frame model.

## Coordinate spaces

Canvas-space content moves and zooms with the viewport. Ordinary application
windows, their decorations, desktop items, and canvas-attached shell controls use
this space. Frame-attached controls and groups stay in screen space. Fullscreen,
focused view, and temporary search/switcher previews have explicit presentation
policies rather than inheriting the ordinary canvas transform.

Changing placement must preserve the identity of the window or control. A
preview or frame attachment is a presentation of the same content, so closing
search or returning to the canvas does not restart its application. The compositor
uses one displayed projection for drawing and inverse input transforms. See
[rendering](RENDERING.md), [size units](SIZE_UNITS.md), and
[window management](WINDOW_MANAGEMENT.md).

## UI frameworks

The shell and Settings use Xilem and Masonry as Wayland clients:

```text
native UI client -> Wayland buffer -> compositor -> display backend
```

They own their widgets, text editing, layout, accessibility semantics, and local
view state. The compositor owns Wayland protocols, placement, stacking, viewport
transforms, focus, input routing, and compositor animations. Requests from clients
express intent; they do not transfer that authority out of the compositor.

Masonry also rasterizes compositor titlebar text and vector icons into cached
strips. This is a narrow embedding of the widget/rendering layer. The compositor
retains its calloop event loop and GLES renderer rather than handing its runtime
to a full application UI driver.

Use a shell client for a new interface that needs substantial forms, text entry,
menus, scrolling, or reusable widgets. Keep effects requiring the desktop's window
stack, clipping, or placement in the compositor. The two can cooperate without
moving application UI state into the compositor core.

The [lock client](SESSION_LOCK.md) is a separate GTK4 process using
`GtkPasswordEntry`, `gtk4-session-lock`, and fixed PAM authentication. It acquires
`ext-session-lock-v1` surfaces, not ordinary XDG toplevels. Shared palette tokens
and exported vector glyphs keep it visually related to the shell. Its background
samples only the user's wallpaper; application pixels never enter that client.
The compositor supplies the opaque fallback and input isolation. `lock-preview`
shows the widgets without authentication or unlock capability.

## Shared visual system

`hearthspace-ui-components` owns palette tokens, native vector glyphs, reusable
controls, and material treatments. The default is warm light UI with an orange
accent. Settings offers Light/Dark and Orange, Rose, Violet, Blue, Teal, or Olive.
Photos and third-party app artwork retain their own colors.

Use a shared `Palette` and `hearthspace_properties(palette, style)` rather than
inventing local theme state. Palette values are ordinary Masonry properties.
Windows retain their widget trees and replace cached defaults when appearance
changes, preserving focus, open controls, and unfinished text. Custom painters
must invalidate paint when the palette property changes. Shell glass uses
`Palette::for_glass()` to adjust foreground contrast and dark hover treatment.

Color should identify action, selection, activity, or focus. Keep destructive
actions distinct across accents. A quieter palette must not make status or
keyboard focus depend on color alone. The default dotted field provides a visual
anchor while navigating; its field and dot colors follow Light/Dark independently
of the accent. Changing modes retains decoded user wallpapers.
See [Settings](SETTINGS.md#linux-appearance-publication) for application appearance
preferences published through the desktop portal and XSettings.

Keep shell copy tied to a decision. A download size, error, unread status, or the
consequences of removal earns its place. A selected outline usually does not need
a second caption announcing selection. Use short tooltips for unfamiliar actions
and accessible names for icon-only controls. Longer explanations belong in
requested details or documentation, not permanent text over the canvas.

## Materials and light

The native material system uses tinted surfaces, restrained edge highlights, and
soft shadows. The compositor can soften the desktop beneath a surface while the
client paints its tint and controls over it.

| Surface role | Purpose |
| --- | --- |
| Ordinary surface | A window or panel that remains related to the content beneath it |
| Raised surface | A toolbar, menu, dialog, or monitor preview that needs a clearer boundary |
| Nested layer | A card, field, or secondary control inside an existing surface |

Give a group or merged bar one outer backing. Its members remain sharp above it;
repeating a complete glass tile around every child makes the group unnecessarily
opaque. Keep the toolbar's hover quieter than the hovered control. Active or
expanded controls use a stronger accent state, while keyboard focus has a distinct
outline. Pointer interaction must not remove selection or material state just
because it hides a keyboard-only focus ring.

Place light near edges, and let nested layers add tint without repeating a strong
gradient. Use shared radii: 5 logical units for details, 10 for controls, 14 for
tiles, and 20 for panels. Inset shapes should follow the parent curve after
accounting for padding and border width. Materials should not wobble, refract, or
stretch decoratively; motion should explain a change of place or state.

Settings uses `settings_button` and `settings_surface` to decorate native controls.
Apply sizing and padding to the native button before wrapping it, and dynamic
`.disabled(...)` to the returned material view so paint and input agree. Shell
buttons use property classes; local background or border values override those
selectors. Prefer the shared styles unless that override is intentional.

### Compositor backdrops

The compositor samples the real stack below a glass surface, excluding its owner's
controls and windows above it. Server-decorated titlebars and the full Settings
window use this path. Overlapping glass resolves from back to front, so each
surface sees completed glass below it. Frame controls and their dropdowns likewise
split backdrop composition from client-drawn controls.

Backdrop textures include the sampling margin and use damage tracking to refresh
changed content. Window backdrops use a 40-logical-unit Gaussian on smaller GPU
textures, then resolve their rounded edge at output density. Output-edge clamping
avoids dark borders. These effects require no CPU pixel readback. If shader
creation fails, the affected blur path is disabled and the client tint remains.

The locked scene does not sample these desktop backdrops. Fullscreen content also
uses its separate presentation path. Preserve those exclusions when adding another
glass owner.

### Client-side limits

Settings' internal choice menus and AI confirmation use an opaque, precomposed
raised backing. Masonry does not supply the desktop backdrop filter inside that
client window. The compact category menu uses a translucent raised tint; the AI
modal dims the underlying client controls. These are distinct from the actual GPU
blur behind the Settings window.

Photo backgrounds use stronger ordinary surface fills and adjusted text contrast.
Check them over both bright and dark regions; a surface that reads well over the
default dots can lose its boundary over a photograph.

## Window decorations and app identity

Wayland toplevels default to client-side decorations. Server-side decoration
negotiation enables Hearthspace's titlebar and rounded frame. Keep the titlebar
and body inside one window boundary; its border, focus treatment, and shadow belong
to the window as a whole.

Server decorations use a 44-logical-unit header with an app icon, centered grip,
and 32-unit maximize/restore and close controls. Masonry caches foreground text
and icon strips at the highest intersecting output density. Geometry-only resizing
repositions and clips those strips; changed text, artwork, control state, palette,
or density invalidates the relevant cache. GPU elements draw the growing material,
divider, grip, border, and shadow.

The compositor clips server-decorated content to a 19-unit inner radius and draws
a one-unit border with a 20-unit outer radius. Active frames use the chosen
accent. Frames sit at their window's stack position beneath its popups and higher
windows; fullscreen content and shell panels do not receive application frames.

Client-decorated windows retain their own pixels and corners. A rectangular rim
and shadow are added only when one mapped root surface establishes opaque coverage
of the full window geometry. Rounded, translucent, and multi-surface clients
receive no invented rim or corner clipping. Input regions and partial opaque hints
are not reliable corner-radius declarations.

## Icons and artwork

Use the shared native glyph set for Hearthspace controls. It uses a 24-by-24 grid,
square caps, miter joins, and optical stroke widths: 1.75 at sizes up to 16, 1.5
between 16 and 32, and 1.25 from 32 upward. Let the control supply its color and
interaction state. Decorative glyphs stay out of the accessibility tree; use
`.label(...)` when the icon supplies an icon-only button's name.

Dedicated filled variants exist for application and document glyphs. Use their
solid geometry rather than filling an outline indiscriminately: doing so can
close the gaps that make the symbol readable. Inspect glyphs at the actual control
size and output density, in both color modes.

Third-party applications keep their catalog artwork across the launcher, dock,
desktop, window titles, and previews. Running and selection indicators belong to
the shell around that artwork. Do not recolor every app into the shell accent or
substitute a Hearthspace control glyph for an application's identity. Distinct
actions such as Change layout, Overview, Arrange, and Apps should retain distinct,
reused glyphs wherever they appear.

## Responsive layouts

Adapt to logical space without overwriting the user's saved placement. At output
widths of 760 logical units or less, unchanged default top corners can form a top
bar and unchanged bottom corners a bottom bar. Custom attachments retain their
chosen positions. Compact bars are temporary projections of the saved layout,
with one backing and scrollable contents where needed. Side attachments keep their
icons upright. See [shell layout](SHELL.md).

Settings uses the client window's width, independently of output width. Below 560
logical units, its sidebar becomes a 49-unit header with a category menu. Selecting
a category or pressing Escape closes the menu and returns focus to its trigger.
Tab follows the header, navigation, and page controls without a focus trap.
Resizing retains the page widgets and unfinished input. Its minimum client size
is `400 × 420` logical units.

Bound popups to the available viewport and scroll long content inside them.
Compact layouts must retain usable touch targets and keyboard access instead of
merely shrinking controls. Hover can explain an action, but must not be the only
way to reach it.

## Motion and interaction ownership

[Desktop items](DESKTOP_ITEMS.md) use native client surfaces with compositor-owned
placement and input. [Canvas search](CANVAS_SEARCH.md) reuses live surfaces and
normal window rendering with temporary transforms. The
[layout editor](SHELL.md#change-layout) uses native controls, groups, tooltips, and
dialogs while the compositor owns capture, guides, clipping, and placement.

When a toolbar turns, its controls, grip, outlines, shadows, and glass move as one
composition. The last complete composition follows the grabbed point while the
native controls reflow; the turn begins after the required final sizes commit.
Input uses the last displayed pose, including when a turn is interrupted. Preserve
that continuity in both directions and honor reduced-motion preferences.

Keyboard, pointer, and touch need the same action semantics, with separate
sequence ownership. A cancelled press must not activate a replacement control on
release. A direct-touch sequence must not also produce a mouse click. See
[gesture ownership](PINCH_ZOOM.md#application-touch-and-pointer-fallback).

## Checking a UI change

Choose a user action and its visible result. Placement changes should preserve
window identity, text, selected pages, and focus. Exercise cancellation and the
next interaction, not only the successful endpoint. For motion, inspect the
grabbed point and intermediate geometry as well as the final rectangle.

Check Light/Dark, another accent, default dots and a photo, narrow layouts, and
relevant output scales. Hover a container and its child separately. Check keyboard
focus, touch activation, accessible names, and long labels. Include overlapping
windows and grouped controls so doubled backings or incorrect stack sampling are
visible.

Use deterministic tests for policy and geometry, composed client/compositor tests
for state and input ownership, and [native acceptance](NATIVE_TESTING.md) for real
presentation and device routing. A visual adjustment does not need a new test that
merely repeats its color constant; it still needs inspection in the running UI.
Follow [development](DEVELOPMENT.md) for the authorized verification workflow.
