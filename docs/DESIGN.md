# A canvas inside a frame

Imagine spreading a few documents across a large table. The plan sits beside
the budget; reference material has its own space. You can look around without
having to pack everything away each time your attention changes.

Hearthspace uses that spatial idea for real applications. The **canvas** holds
your work. The **frame** keeps chosen things within reach as you move around it.
This guide explains the design intent; the feature guides describe current
behavior and exceptions.

## Location should be useful

Panning changes which part of the canvas you see. It doesn't change where a
window belongs. Zoom helps you move between an overview and the work itself.
A window beyond the display remains part of the desktop, reachable through
search, the dock, the map, or the window switcher.

The default dots move with the canvas, giving an otherwise empty area a visible
reference. Keep the surface quiet enough for applications to provide the
content. Permanent instructions, decorative headings, and status banners
shouldn't compete with the work.

Spatial persistence remembers an arrangement, not an application's contents.
Returning to saved positions doesn't reopen every app or restore an unsaved
document. [Window management](WINDOW_MANAGEMENT.md) explains that boundary.

## The frame is configurable

An attached object stays along the chosen screen edge while the canvas moves.
A reference window, a folder shortcut, and the app dock can all be useful there.
Changing placement doesn't create a second copy or a new kind of object.

The four corners give the first session an understandable arrangement:
notifications above left, system controls above right, canvas tools below left,
and apps below right. Those positions are defaults. Avoid designing a feature
that only works in its original corner.

**Change layout** makes placement deliberate. Ordinary window movement doesn't
suddenly pin a window because it approached the screen edge. In the editor,
dragging to an edge attaches an object; dragging into the interior returns it
to the canvas. Related attachments can share a group and one surface.

A pinned window recedes to a strip, peeks on hover, and opens on activation.
The user can keep a reference close without giving it a permanent block of
screen space. Removing an attachment returns personal work to the canvas;
closing a pinned window is a separate, confirmed action.

Controls must remain recoverable. Hidden controls stay in search, and Settings
provides layout and canvas-tool recovery. See [shell layout](SHELL.md#change-layout)
for placement, grouping, and the safeguard around hiding Canvas tools.

## Temporary arrangements shouldn't overwrite chosen ones

Search and the window switcher borrow the canvas to help you choose. They show
actual window content and recognizable app artwork, while their display positions
remain separate from the saved arrangement. Canceling returns to the previous
view. Choosing a result takes you to that work.

Keep movement continuous enough to explain where the selected object went.
Reduced motion should reveal the same final state immediately. A decorative
animation must not delay input, change a target's identity, or make a release
activate a different control.

## Give each layer a job

Background objects sit beneath ordinary windows. Raised canvas items and
controls still belong to the canvas. Frame attachments remain at the edge,
and temporary panels appear above the work they need to control.

When an object is completely covered, a small visible portion can help the user
find it again. Temporary displacement should preserve its chosen home where
that object's rules allow it. Deliberate movement establishes a new home.
[Desktop items](DESKTOP_ITEMS.md) and [window management](WINDOW_MANAGEMENT.md)
explain the differences between item and window settlement.

Thin translucent surfaces, blur, edge highlights, and restrained shadows separate
shell controls from the work beneath them. Shared icons and consistent focus,
hover, and selected states help controls belong together even after rearrangement.
App artwork and application interiors keep their own identity. Technical material,
sizing, and icon guidance lives in [UI](UI.md).

## Explain the decision at hand

Use words where they help someone act: an error, an unavailable device, a download
size, or the consequence of deleting something. Avoid repeating an outline with
a selected-item count or adding a heading above an already recognizable control.

Compact presentation still needs accessible names, keyboard focus, and usable
touch targets. Hover is a clue, never the only route to an action. Editable text
must remain editable, and applications keep responsibility for selecting their
own content.

Native features must report real state. A theme doesn't justify simulated
battery readings, a fake network connection, or a password shortcut. Where a
system integration isn't available, communicate that honestly and retain a
working route to the user's existing tools.
