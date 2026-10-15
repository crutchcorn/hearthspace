# Three-Finger Pan and Zoom

Spread three fingers on a touchpad or touchscreen to zoom into the canvas;
bring them together to zoom out. Slide all three fingers left, right, up, down,
or diagonally to pan the canvas in that direction. No modifier key is required.
The content follows the fingers, and the toolbar remains fixed in screen space.

Pan and zoom can happen together during a pinch. Touchpad gestures start around
the pointer; touchscreen gestures start at the center of the three contacts.
That original canvas point follows the gesture's movement, within integer
coordinate rounding. Zoom stays within 0.5×–2×; panning still works at either
limit. Rotation does not rotate the canvas. These gestures control the desktop
canvas even when the pointer is over an application.

For combined touchpad movement, begin by pinching or spreading, then slide the
three fingers while continuing the pinch. Starting with all fingers moving in
the same direction can be recognized as a swipe, which pans without zooming.

Native touchpads use Smithay's existing libinput swipe and pinch events. The
swipe path pans without changing scale; the pinch path uses both scale and
center movement. Touchscreens use raw touch contacts, grouped by device and
frame, to track their center and spread. No new protocol or vendored backend is
required. Shell zoom buttons and Super+scroll
remain available.

An actual three-finger pan or zoom restores maximized applications to their saved
floating size and canvas position, so they move with the desktop. The focused
application or its dialog is revealed first if its saved position is off-screen.
When a shell control has focus, its output's topmost maximized application is
revealed instead, while keyboard focus stays on the shell control.
Fullscreen applications stay fullscreen. Shell zoom buttons and Super+scroll use
the same rule for zoom. Starting a gesture without changing the view does not
restore maximized windows. A clamped pinch without translation also leaves them
maximized; a pan at the zoom limit restores them.

The pinned Smithay/winit backend only delivers two-finger touchpad pinch events
and no touchpad swipe events, so three-finger touchpad navigation requires the
native backend. Nested touchscreen events use the same recognizer, with an
adapter for winit's unframed physical
coordinates. Nested X11 does not supply touchpad pinch events.

## Interaction and Cancellation

A canvas gesture does not begin during a window drag, resize, pointer or keyboard
grab (including popup grabs). An active touchscreen sequence blocks touchpad
gestures. Exactly three fingers are required; contacts from separate touchscreens
are never combined. Updates from another device or gesture type cannot change
the active gesture.
Touchscreen positions use the compositor's existing primary-output mapping.

Touchscreen taps click on release. A single contact moving at least eight logical
pixels starts a drag from its original position. Adding another contact before
that point suppresses the click and allows three-finger navigation. Adding contacts
after a drag has started releases the drag and suppresses the rest of that touch
sequence. Two-finger touchscreen gestures do not pan, zoom or click.

The first normal lift ends touchscreen navigation at its current view. A fourth
contact or touch cancellation cancels it. Remaining contacts cannot start another
gesture or become a click until all fingers have lifted.

Ending normally retains the current view. Cancelling an active gesture, removing
its device, losing nested-window focus, or suspending the native session restores
the view from that gesture's beginning and re-maximizes windows restored by that
gesture. Libinput may end a gesture normally before reporting device removal;
that completed gesture retains its final view.
A mouse press, shell/scroll navigation, output geometry change, or newly observed
grab instead ends the gesture at its current view, letting the newer interaction
proceed. Later events from the interrupted gesture cannot restore an obsolete
viewport or re-maximize those windows.

## Implementation and Testing

Smithay reports absolute scale and per-event logical center deltas. Touchpad
deltas accumulate as floating-point values without output-scale division;
touchscreen translation is the current center minus its initial center. Each
update transforms the original viewport using the total translation and absolute
scale, rounding only the resulting canvas offset. Small movements therefore
accumulate, and reversing direction does not accumulate rounding errors. Pointer
focus and cursor hit testing refresh when the viewport changes. If the first
meaningful movement or scale change reveals a previously maximized window, the
gesture's anchor is rebased to
that view; cancellation still retains the original viewport and window states.

[Libinput gesture semantics](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html)
provide recognition and begin/update/end events. The
[Wayland pointer-gestures protocol](https://wayland.app/protocols/pointer-gestures-unstable-v1)
is for forwarding gestures to applications; canvas navigation is owned by the
compositor and does not require advertising that protocol.

Unit regressions cover cardinal/diagonal translation, combined pan and zoom,
fractional motion, translated anchoring, bounds and reversal,
invalid updates, device isolation, contact batching, pointer arbitration, normal
completion, and cancellation. For the kernel-to-compositor path and visible
native presentation, use the optional
[native gesture acceptance tests](../CONTRIBUTING.md#optional-native-pinch-testing).
They create temporary virtual touchpad and touchscreen devices and exercise the
normal libinput path. `scripts/pan-smoke` checks visible translation and combined
scaling; `scripts/pinch-smoke` checks zoom, contact arbitration and device removal.
These tests validate the input and rendering pipeline, without emulating a
person's physical finger feel on the hardware.
