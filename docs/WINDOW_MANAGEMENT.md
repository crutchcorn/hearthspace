# Window management

Hearthspace keeps ordinary application windows on a shared canvas. Window
management makes those windows reachable without changing their saved canvas
positions, and offers conventional fullscreen and maximized views for focused
work.

## Coordinates and window states

A floating window has a canvas position and a client content size. Panning and
zooming change the view of that window. Pointer input uses the inverse of the
same transform used to render it.

A maximized or fullscreen window belongs to one output and uses that output's
logical coordinates at native scale. Canvas zoom does not shrink the window or
change its input coordinates. Maximized windows reserve space for the shell bar
and any compositor title bar. Fullscreen windows cover the output and suppress
those decorations. A smaller fullscreen client buffer is centered against an
opaque black background.

Entering either state preserves the floating position and content size.
Fullscreen may be layered over maximized state: leaving fullscreen returns to
maximized, and restoring clears both states. The original floating size remains
available until the client commits the restore configure. Repeated state
changes must not accidentally save a still-fullscreen buffer as the floating
size.

Output changes reconfigure pinned windows. If their output disappears, they
move to the primary output. Their saved canvas position remains intact.

## Focus, dialogs, and recovery

A newly mapped application receives focus. Activation follows actual keyboard
focus, including menus associated with that application. Closing or unmapping
the focused window returns focus to a surviving application. Shell chrome and
X11 override-redirect surfaces do not participate in normal window cycling.

Focusing an offscreen floating window reveals it by moving the viewport. Its
canvas position is unchanged. Focusing another application can raise it above a
fullscreen window; returning to the fullscreen application raises its dialog
family with it. Activating the launcher exposes the shell bar even during
fullscreen.

Transient dialogs open near their parent. A dialog of an output-pinned window
inherits the output and native scale, including through a chain of dialogs.
Restore the parent before moving those dialogs around the canvas. Popup menus
use the client's Wayland positioner constraints to flip, slide, or resize into
the available output area. Active popup grabs retain input authority; window
actions return an error until the popup is dismissed.

## Keyboard control

| Shortcut | Action |
| --- | --- |
| Super+Return | Open a terminal |
| Super+Space | Focus launcher search |
| Alt+Tab / Alt+Shift+Tab | Focus the next / previous application |
| Alt+F4 | Request that the focused application close |
| Super+arrow keys | Pan the canvas |
| Super+0 | Reset zoom and reveal the focused window |
| Super+F | Toggle fullscreen |
| Super+M | Toggle maximized state |

Application shortcuts such as F11 continue to reach the client. Real and
synthetic keyboard events share the same compositor shortcut policy.

## Agent control

Use `scripts/hearthspace-agent --session SESSION windows` to obtain stable IDs
for the current session. The `focus`, `close`, `move`, `resize`, `fullscreen`,
`maximize`, and `restore` actions target those IDs. `move` takes canvas
coordinates; `resize` takes client content dimensions in logical pixels.
Fullscreen and maximized windows must be restored before moving or resizing.

Window metadata distinguishes canvas position, screen geometry, client content
size, focus, and presentation state. The `visible` field describes intersection
with an output; it does not promise that another window is not covering the
content. A successful close action means the close request was delivered, so
observe the window list to confirm that the application actually exited.

See [native testing](./NATIVE_TESTING.md) for session ownership and capture
workflows, and [size units](./SIZE_UNITS.md) for output scale and logical pixels.

## Remembered layout

Native sessions save the viewport and ordinary window canvas positions on a
normal exit, including logout or SIGTERM. A new session restores the viewport;
when you open applications, their windows reuse remembered positions in opening
order (up to eight windows per application). Transient dialogs keep following
their parents. Application settings remain responsible for window content and
size; Hearthspace does not reopen apps or save their titles, documents, or text.

The private file is `$XDG_STATE_HOME/hearthspace/layout.json`, falling back to
`~/.local/state/hearthspace/layout.json`. It retains at most 128 placements.
Invalid files are ignored. A crash can lose layout changes since the last normal
exit. Set `HEARTHSPACE_LAYOUT_PATH` to an absolute file to choose another layout,
or `off` to disable persistence. Nested and headless sessions only save when an
explicit path is supplied. The native agent harness uses `layout.json` inside
its artifact directory so tests do not alter the user's normal layout.

## Contributor rules

- Render, hit testing, output membership, and metadata must use the same
  per-window transform.
- Keep saved canvas placement separate from output-pinned presentation.
- Respect client grabs and keep focus activation synchronized with the seat.
- Exercise real Wayland and X11 clients at non-unit canvas zoom when changing
  window state, geometry, or transient placement.
