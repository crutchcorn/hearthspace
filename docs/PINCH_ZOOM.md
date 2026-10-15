# Pinch Zoom

Spread three fingers on a touchpad or touchscreen to zoom into the canvas;
bring them together to zoom out. No modifier key is required. Touchpad zoom is
anchored at the pointer. Touchscreen zoom is anchored at the initial center of
the three contacts. That canvas point stays fixed, within integer coordinate
rounding, and the toolbar remains fixed in screen space.

Zoom follows the fingers immediately, within the existing 0.5×–2× limits. Gesture
translation and rotation do not pan or rotate the canvas. This gesture controls
the desktop canvas even when the pointer is over an application.

Native touchpad zoom uses Smithay's existing libinput pinch events. Touchscreen
zoom uses Smithay's raw touch contacts, grouped by device and frame. No new
protocol or vendored backend is required. Shell zoom buttons and Super+scroll
remain available.

An actual canvas zoom restores maximized applications to their saved floating
size and canvas position, so they zoom with the rest of the desktop. The focused
application or its dialog is revealed first if its saved position is off-screen.
When a shell control has focus, its output's topmost maximized application is
revealed instead, while keyboard focus stays on the shell control.
Fullscreen applications stay fullscreen. Shell zoom buttons and Super+scroll use
the same rule. Starting a pinch, keeping its scale unchanged, or pushing beyond
a zoom limit does not restore maximized windows.

The pinned Smithay/winit backend only delivers two-finger touchpad pinch events,
so three-finger touchpad zoom requires the native backend. Nested touchscreen
events use the same recognizer, with an adapter for winit's unframed physical
coordinates. Nested X11 does not supply touchpad pinch events.

## Interaction and Cancellation

A pinch does not begin during a window drag, resize, pointer or keyboard grab
(including popup grabs). An active touchscreen sequence blocks touchpad zoom.
Only three-finger pinches are used; contacts from separate touchscreens are never
combined. Updates from another input device cannot change the gesture.
Touchscreen positions use the compositor's existing primary-output mapping.

Touchscreen taps click on release. A single contact moving at least eight logical
pixels starts a drag from its original position. Adding another contact before
that point suppresses the click and allows a three-finger pinch. Adding contacts
after a drag has started releases the drag and suppresses the rest of that touch
sequence. Two-finger touchscreen gestures do not zoom or click.

The first normal lift ends touchscreen zoom at its current view. A fourth contact
or touch cancellation cancels it. Remaining contacts cannot start another gesture
or become a click until all fingers have lifted.

Ending normally retains the current view. Cancelling an active gesture, removing
its device, losing nested-window focus, or suspending the native session restores
the view from that gesture's beginning and re-maximizes windows restored by that
gesture. Libinput may end a gesture normally before reporting device removal;
that completed gesture retains its final view.
A mouse press, shell/scroll navigation, output geometry change, or newly observed
grab instead ends the pinch at its current view, letting the newer interaction
proceed. Later events from the interrupted gesture cannot restore an obsolete
viewport or re-maximize those windows.

## Implementation and Testing

Smithay reports scale relative to the gesture's beginning. Each update uses the
saved initial viewport and pointer anchor, so repeated updates and changes of
direction do not accumulate scale or rounding errors. Pointer focus and cursor
hit testing are refreshed when the viewport changes. If the first scale change
reveals a previously maximized window, the gesture's zoom anchor is rebased to
that view; cancellation still retains the original viewport and window states.

[Libinput gesture semantics](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html)
provide recognition and begin/update/end events. The
[Wayland pointer-gestures protocol](https://wayland.app/protocols/pointer-gestures-unstable-v1)
is for forwarding gestures to applications; canvas zoom is owned by the
compositor and does not require advertising that protocol.

Unit regressions cover absolute scaling, fixed anchoring, bounds and reversal,
invalid updates, device isolation, contact batching, pointer arbitration, normal
completion, and cancellation. For the kernel-to-compositor path and visible
native presentation, use the optional
[native pinch acceptance test](../CONTRIBUTING.md#optional-native-pinch-testing).
It creates temporary virtual touchpad and touchscreen devices and exercises the
normal libinput path. It checks visible scaling and the distinct pointer/centroid
anchors, and does not emulate a person's physical finger feel on the hardware.
