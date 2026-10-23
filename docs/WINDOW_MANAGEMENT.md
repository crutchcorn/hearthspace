# Window management

Hearthspace keeps ordinary application windows on a shared canvas. Navigation
reveals existing work, while canvas peeking keeps fully covered work discoverable.
Fullscreen and maximized views provide room for focused work.

## Coordinates and window states

A floating window has a canvas position and a client content size. Panning and
zooming change the view of that window. Pointer input uses the inverse of the
same transform used to render it.

New application windows open centered in the current output's usable view and
receive keyboard focus without moving the canvas. The actual window geometry,
canvas zoom, and shell bar determine placement. A window larger than the usable
view starts at its top-left so its controls remain reachable. Dialogs stay
centered on their parents. Alt-Tab, search selection, and explicit focus actions
still navigate to existing windows.

Double-clicking a server title bar or a cooperating GTK title bar expands the
window on the canvas. At output widths above 760 logical pixels, its outer bounds
leave 24 pixels at each side
and 96 at the top and bottom; smaller outputs use 12 and 78. The clearances stay
stable when frame controls move or hide. Maximization fits the current view,
including its zoom, then keeps that canvas size and location as the view pans
or zooms. Double-click again to restore the original size and position.
Individual maximized windows must be restored before moving or resizing them.
Dragging a selection of multiple windows moves each maximized member's visible
bounds and saved restore position together.

The server title bar's expand button enters **focused view** instead. The window
fills its output at native scale, retains its title bar, and uses square corners.
The canvas stays still while the application receives input. Click the button
again, or double-click the header, to return to ordinary canvas maximization;
another header double-click restores the original window. Super+Shift+F offers
the same focused-view action from the keyboard. Frame-attached windows use both
header actions to expand or tuck at their existing edge.

Application maximize requests also enter focused view. Their restore requests
return to the original floating size. This includes native Wayland and XWayland
applications, whether the request comes from a button, a menu, or application
code. Repeated maximize requests preserve the current maximized mode. A background
request does not take focus or suspend canvas navigation; its focused view starts
when the application becomes active.

GTK clients that send `gtk_surface1.titlebar_gesture` get the separate header
double-click action: canvas maximize/restore, or focused view back to canvas
maximization. Other client-drawn headers may send the same generic maximize
request for both the button and double-click; both enter focused view in that
case. Super+M and Super+Shift+F remain separate actions for every application.
Hearthspace does not infer a header gesture from click timing or coordinates.
GTK's default right/middle header actions remain no-ops; customized right/middle
actions are not yet supported by the extension. The
[protocol contract](../crates/hearthspace-compositor/protocols/README.md) documents
its supported requests and input checks.

Focused view hides the shell until the pointer approaches a corner (within 72
logical pixels of both adjoining edges). Controls remain visible while in use,
with a 450 ms grace after leaving them; a touchscreen corner tap grants 2.5
seconds. Server-decorated header buttons do not trigger this reveal. Super+F6
reveals and focuses shell controls for keyboard use. Escape dismisses a shell
panel or returns keyboard focus to the application; it does not exit focused
view. Activating another app, canvas search, or the layout editor leaves focused
view and keeps the previous app maximized on the canvas.

Application-requested fullscreen (such as F11) and Super+F remain separate.
Fullscreen uses the entire output at native scale and suppresses server
decorations. A smaller fullscreen client buffer is centered against an opaque
black background. Leaving fullscreen returns to the preceding focused or
maximized state. Restore clears all presentation states. The original floating
geometry remains available until the client commits the restore configure;
rapid state changes cannot replace it with a pending enlarged buffer.

Focused and fullscreen windows follow their output's configuration. If it
vanishes, they move to the primary output. Temporary maximized and focused
geometry does not overwrite the saved floating position.

The session supplies GTK defaults that expose maximize/restore and close controls
in application title bars, including Firefox. Existing user button-layout
preferences take precedence. These defaults live only in the owned session
runtime and are inherited by directly launched and portal-activated applications.
They do not add unsupported minimize behavior or modify another desktop's settings.

Applications that request compositor decorations receive a 44-logical-pixel
title bar with maximize/restore and close controls. Each control is 32×32, with
a four-pixel gap and six pixels of padding at the right edge. Painting and hit
testing use the same rectangles; canvas zoom scales both, while output-pinned
windows retain their native logical size.

Click or tap a control to activate it on release. Releasing off the pressed control,
losing its window, changing its geometry, or cancelling the input sequence
prevents activation. A close requests that the application exit; it does not
force termination. Drag empty title-bar space to move a floating window, or
double-click it to toggle canvas maximize/restore, as with Super+M. The expand
button toggles focused view, as with Super+Shift+F. Both preserve the original
floating geometry.
Client-drawn title bars retain their own controls; GTK's explicit title-bar
gesture protocol lets the compositor distinguish their header double-clicks.

Hold **Alt** while moving a floating window to align its visible top-left corner
with the canvas dot grid. Hold Alt while resizing an edge or corner to align
the edges being dragged; the opposite edges stay anchored. Alignment includes
the compositor title bar and excludes client shadow margins. The grid stays in
canvas coordinates as the view pans or zooms, and uses the same spacing as
[desktop icons](DESKTOP_ITEMS.md). Pressing or releasing Alt updates an active
move or resize without requiring another pointer movement. Application size
limits take precedence when a requested size cannot align with the grid.
Terminals such as Foot may also round the final size to character cells on
release; the opposite edges remain anchored to the actual accepted size.

## Focus, dialogs, and recovery

### Covered canvas work

Moving and resizing allow overlap throughout the gesture. After release, a fully
covered window slides to the nearest position exposing a 24-pixel square. Several
higher windows can jointly cover it. Even a small visible strip prevents movement;
peeking preserves window sizes and stacking order. Opening or maximizing a window
also settles covered work. Windows keep the resulting canvas positions, including
the translated restore position of a maximized window.

Desktop icons and free canvas controls expose a 32-pixel patch when covered.
Icons use their 56-pixel artwork, excluding transparent padding and labels, to
decide coverage. They remember their first position before automatic displacement
and return when it becomes clear. A window must clear the complete home bounds,
including labels, before an icon returns. Moving an icon or control yourself
establishes a new home. Displaced positions and their homes survive session saves.

Change layout placements make room around the dropped object's footprint.
Background objects and controls leave ordinary foreground windows fixed. A
deliberate Background placement adopts the affected neighbors' displaced positions
as new homes. Unrelated temporary displacements retain their original homes.
Frame attachments do not participate in canvas peeking.

Automatic travel lasts 300 ms and respects reduced motion. Panning and zooming
preserve the settled canvas coordinates. Search, overview, and window switching
own their temporary presentation; entering focused view or fullscreen does not
push surrounding work aside. The geometry policy lives in
`hearthspace-common::canvas_peek`; the compositor applies final positions and
projects only the temporary animation offset for rendering and input.

### Activation and recovery

A newly mapped application receives focus. Activation follows actual keyboard
focus, including menus associated with that application. Closing or unmapping
the focused window returns focus to a surviving application while preserving
the canvas position and zoom, even if that application is offscreen. Shell
chrome and X11 override-redirect surfaces do not participate in normal window
cycling.

Explicitly focusing an offscreen floating window reveals it by moving the
viewport. Its canvas position is unchanged. Focusing another application can
raise it above a fullscreen window; returning to the fullscreen application
raises its dialog family with it. Activating the launcher exposes the shell bar
even during fullscreen.

Transient dialogs open near their parent. A dialog of an output-pinned window
inherits the output and native scale, including through a chain of dialogs.
Restore the parent before moving those dialogs around the canvas. Popup menus
use the client's Wayland positioner constraints to flip, slide, or resize into
the available output area. Active popup grabs retain input authority; window
actions return an error until the popup is dismissed.

## Canvas map

The **Canvas map** button in Canvas tools opens the [canvas map](CANVAS_MAP.md), with a live
zoomed-out minimap and icon zoom controls. Drag its orange viewport area to move
around the canvas without changing the zoom level or window positions.

## Window switcher

Alt+Tab gathers live windows into a centered spatial grid and selects the most
recently used alternative. Keep Alt held and press Tab again to continue; add
Shift to move backward. Releasing Alt, pressing Enter, or clicking/tapping a preview
activates it. Escape or an empty-background click/tap cancels switching and restores
the original presentation.

An inward pinch opens the same switcher persistently: use four fingers on a
native touchpad, or four/five on a touchscreen, then lift. The grid opens only
after a successful gesture end, selects the next most recently used window, and
stays open after release. Repeated inward pinches keep the current selection.
Crossing the pinch threshold and returning before lifting, or cancelling the
gesture, leaves it closed. Click or tap a preview, or press Enter, to activate the selection; Escape,
a background click/tap, or a completed outward pinch dismisses it. A drag across
a preview does not activate it.

Switcher pinches also work in focused view. Dismissal retains the focused app;
activating another preview leaves the previous app maximized on the canvas.

Swipe four fingers left on a native touchpad, or four/five on a touchscreen,
to select the next window. Swipe right to select the previous window. The same
grid remains open while the gesture is held. Continued movement steps through
additional windows, and reversing direction moves back. Lift to keep the chosen
window. See [canvas gestures](./PINCH_ZOOM.md) for recognition thresholds and the
native backend's five-finger touchpad limitation.

The grid opens on the output under the pointer and includes windows outside the
current canvas view. The original application retains focus while previews are
shown. Cycling only changes the accent outline and the title badge below the selected preview;
hovering does not change selection. The viewport and saved window positions,
sizes, zoom, and presentation states remain unchanged until activation. Existing
window buffers and decorations animate into the grid over 360 ms, while desktop
icons move out of view and fade. Corner controls return when switching ends.
Activation reveals the selected window and animates from its preview into the
final view. Fullscreen and maximized clients keep their presentation state.
The shared search motion preference honors reduced animations.

The initial order is front to back with the focused app first, and stays fixed
for the interaction, with wrapping at either end. All previews fit into
rows; closed windows are removed and new windows join the next interaction.
Typing, arrow keys, and scrolling are consumed while the grid is open, and canvas
pan and zoom are suspended. Preview clicks never reach application contents. The switcher
cannot open during a move, resize, popup grab, or session lock. The keyboard and
gestures cannot take over each other's held switcher. Locking or losing the native
VT dismisses it and clears the held input state. Unplugging the keyboard cannot leave Alt or its overlay
stuck; a backend-synthesized release keeps the selection, otherwise removal
cancels the held switch.

## Touchscreen movement and resizing

Drag a floating window's title bar with one finger to move it, then lift to drop
it. This works with compositor decorations and application-drawn title bars,
including Firefox's empty tab-strip area and GTK headerbars such as Calculator's.
Drag a resizable window's edge to resize it. Restore maximized windows first.

Movement begins after eight logical pixels and includes that initial movement.
The window follows the finger's screen distance at any canvas zoom. Adding a
second contact ends the drag and suppresses that touch sequence until all fingers
lift; touch cancellation or device removal also releases the window. Three-finger
canvas navigation and four-/five-finger switcher pinches are separate gestures.

## Keyboard control

Apps List opens from its dock button or a Meta tap. It searches installed apps
and running windows. [Canvas search](CANVAS_SEARCH.md) opens with Ctrl+K or
Super+K and temporarily arranges matching live objects around its field.
[Shell controls](SHELL.md) describes both entry points and the corner dropdowns.

| Shortcut | Action |
| --- | --- |
| Super+Return | Open a terminal |
| Meta / Super tap | Toggle Apps List |
| Super+Space | Open Apps List |
| Ctrl+K / Super+K | Open canvas search |
| / (desktop focus) | Open canvas search |
| Alt+Tab / Alt+Shift+Tab | Select next / previous application; hold Alt for previews |
| Alt+F4 | Request that the focused application close |
| Super+arrow keys | Pan the canvas |
| Super+F | Toggle application fullscreen |
| Super+Shift+F | Toggle focused view with the title bar retained |
| Super+M | Toggle maximized state |
| Super+L | Lock the native session |
| Ctrl+Alt+F1–F12 | Switch to Linux TTY1–TTY12 in a native session |

While the window switcher is open, Escape dismisses it and Enter activates the
selected window. Super+L remains available. Outside the switcher, application
shortcuts such as F11 continue to reach the client.
Real and synthetic keyboard events share the same compositor navigation policy.
VT switching is a native keyboard binding, handled before application grabs,
search, the window switcher, and the lock screen. It preserves the running desktop
and its lock state; switching back resumes it. On laptops whose function row defaults
to media controls, hold Fn as well to send an F key. Nested and headless sessions
do not switch the host's TTY. Agents can exercise the native binding through an
owned uinput keyboard; control-socket key injection remains inside Hearthspace.
See the [native TTY validation record](VT_TESTING.md) for recovery and lock checks.

## Agent control

Use `scripts/hearthspace-agent --session SESSION windows` to obtain stable IDs
for the current session. The `focus`, `close`, `move`, `resize`, `fullscreen`,
`maximize`, and `restore` actions target those IDs. `move` takes canvas
coordinates; `resize` takes client content dimensions in logical pixels.
Fullscreen, focused, and maximized windows must be restored before moving or resizing.

Unlocked status includes `window_switcher` with its active input source,
selected window ID, output, and preview rectangles. `window_switcher.animating`
lets agents wait for preview motion to finish, including the return animation
after dismissal. Read the state again after windows close or outputs change
before acting on a preview. Switcher state is omitted from status while the
session is locked. See [native testing](NATIVE_TESTING.md) for its status fields.

Window metadata distinguishes canvas position, screen geometry, client content
size, focus, and presentation state. The `visible` field describes intersection
with an output; it does not promise that another window is not covering the
content. A successful close action means the close request was delivered, so
observe the window list to confirm that the application actually exited.

See [native testing](./NATIVE_TESTING.md) for session ownership and capture
workflows, and [size units](./SIZE_UNITS.md) for output scale and logical pixels.

## Remembered layout

Native sessions save the viewport and ordinary window canvas positions on a
normal exit, including logout or SIGTERM. A new session restores the viewport.
Opening an application creates a centered window in that view; stored application
positions do not move newly opened windows offscreen or navigate the canvas.
Transient dialogs keep following their parents. Application settings remain
responsible for window content and size; Hearthspace does not reopen apps or save
their titles, documents, or text.

The private file is `$XDG_STATE_HOME/hearthspace/layout.json`, falling back to
`~/.local/state/hearthspace/layout.json`. It retains at most 128 placements.
Invalid files are ignored. A crash can lose layout changes since the last normal
exit. Set `HEARTHSPACE_LAYOUT_PATH` to an absolute file to choose another layout,
or `off` to disable persistence. Nested and headless sessions only save when an
explicit path is supplied. The native agent harness uses `layout.json` inside
its artifact directory so tests do not alter the user's normal layout.

## Native switcher validation — 2026-10-22

Native switcher acceptance on 2026-10-22 passed all 41 stages in
`/tmp/hearthspace-icon-switcher.1D06Er`, including keyboard switching and physical
touchpad/touchscreen pinch events. A subsequent Intel UHD 620 run at 1920×1080,
scale 1, verified that inward touchscreen pinch leaves the centered switcher
open after release. Escape and background click preserved focus; pointer click
and touchscreen tap on another preview activated that app. Captures and exact
before/after focus assertions are in
`/tmp/hearthspace-gpu-portal.mwspkdj0/switcher-extra`. Both owned sessions were
cleaned up; the latter restored tty2 with no GPU loss or panic logged.

## Contributor rules

- Render, hit testing, output membership, and metadata must use the same
  per-window transform.
- Keep saved canvas placement separate from output-pinned presentation.
- Respect client grabs and keep focus activation synchronized with the seat.
- Keep compositor control presses owned until release, including cancellation.
  Device removal and session recovery must clear their button-source state
  without activating the control or sending an unmatched release to a client.
- Client move/resize requests continue the originating pointer grab, including
  touchscreen pointer emulation. Validate the Wayland seat, button serial and
  window; stale or overlapping requests must not restart an operation.
- Exercise real Wayland and X11 clients at non-unit canvas zoom when changing
  window state, geometry, or transient placement.
