# Pinch Zoom

Spread two fingers on a touchpad to zoom into the canvas; bring them together to
zoom out. No modifier key is required. The canvas point under the pointer when
the gesture starts stays anchored there, within the canvas's integer coordinate
rounding. The toolbar remains fixed in screen space.

Zoom follows the fingers immediately, within the existing 0.5×–2× limits. Gesture
translation and rotation do not pan or rotate the canvas. This gesture controls
the desktop canvas even when the pointer is over an application.

The compositor uses Smithay's existing pinch events from libinput or its nested
Wayland backend. These are touchpad gestures; touchscreen input remains the
existing single-contact pointer emulation. Nested X11 does not supply pinch
events. Shell zoom buttons and Super+scroll remain available.

## Interaction and Cancellation

A pinch does not begin during a window drag, resize, pointer or keyboard grab
(including popup grabs), or an active touchscreen contact. Only two-finger
pinches are used. Updates from another input device cannot change the gesture.

Ending normally retains the current view. Cancelling an active gesture, removing
its device, losing nested-window focus, or suspending the native session restores
the view from that gesture's beginning. Libinput may end a gesture normally
before reporting device removal; that completed gesture retains its final view.
A mouse press, shell/scroll navigation, output geometry change, or newly observed
grab instead ends the pinch at its current view, letting the newer interaction
proceed. Later events from the interrupted
gesture cannot restore an obsolete viewport.

## Implementation and Testing

Smithay reports scale relative to the gesture's beginning. Each update uses the
saved initial viewport and pointer anchor, so repeated updates and changes of
direction do not accumulate scale or rounding errors. Pointer focus and cursor
hit testing are refreshed when the viewport changes.

[Libinput gesture semantics](https://wayland.freedesktop.org/libinput/doc/latest/gestures.html)
provide recognition and begin/update/end events. The
[Wayland pointer-gestures protocol](https://wayland.app/protocols/pointer-gestures-unstable-v1)
is for forwarding gestures to applications; canvas zoom is owned by the
compositor and does not require advertising that protocol.

Unit regressions cover absolute scaling, fixed anchoring, bounds and reversal,
invalid updates, device isolation, normal completion, and cancellation. For the
kernel-to-compositor path and visible native presentation, use the optional
[native pinch acceptance test](../CONTRIBUTING.md#optional-native-pinch-testing).
It creates a temporary virtual touchpad and exercises the normal libinput path;
it does not emulate a person's physical finger feel on the built-in touchpad.
