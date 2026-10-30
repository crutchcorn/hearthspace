# Canvas gestures

Three fingers pan and zoom the canvas. Four fingers on native touchpads, or
four/five on touchscreens, open or dismiss the window switcher with a pinch and
switch windows with a horizontal swipe. These desktop gestures need no modifier
and do not zoom application content.

For a mouse without a trackpad, hold **❖** and left-drag empty canvas, or choose
[Pan canvas](WINDOW_MANAGEMENT.md#pan-canvas-with-a-mouse) for repeated dragging
without a modifier. Hold **❖** while scrolling to zoom.

This guide covers touch controls and input ownership. See
[window management](WINDOW_MANAGEMENT.md#window-switcher) for the switcher's window
presentation and keyboard controls, and [backend availability](#backend-availability)
for input-device limits.

## Three-finger pan and zoom

Spread three fingers to zoom in; bring them together to zoom out. Move all three
in the same direction to pan. Pan and zoom can happen together during a pinch.
Frame-attached shell controls remain in screen space while the canvas moves.

Touchpad navigation starts around the pointer. Touchscreen navigation starts at
the center of the three contacts. That original canvas point follows the gesture's
translation, subject to integer rounding. Zoom stays within `0.5×–2×`; panning
continues at either limit. Rotation does not rotate the canvas.

On a touchpad, begin with a pinch or spread before translating if you want to
combine zoom and pan. Parallel motion can be recognized as a swipe instead;
three-finger swipes pan without changing scale. The native backend receives
Smithay/libinput swipe and pinch events. Touchscreens use contact center and
spread, grouped by device and input frame.

Maximized applications move and scale with the canvas. Their expanded canvas
bounds and restore geometry remain unchanged; navigation does not automatically
restore or recenter them. Shell zoom controls and Super+scroll follow the same
rule. Application fullscreen stays on its output. Focused view blocks canvas
navigation until it is closed or another application or canvas tool is activated.

## Four- and five-finger pinch

Pinch inward and lift to open a persistent window switcher. Use four fingers on
a native touchpad, or four/five on a touchscreen. A successful inward pinch
selects the next most recently used window; repeating it while the switcher is
open retains that selection. Spread the same supported contact count and lift to
dismiss the switcher.

The final spread determines the action:

| Final spread relative to the baseline | Result after a normal end |
| --- | --- |
| At most 80% | Open the switcher if it is closed |
| At least 125% | Dismiss the gesture-opened switcher |
| Between those thresholds | No transition |

Crossing a threshold and returning before lifting does nothing. Cancellation does
nothing, and each completed pinch causes at most one transition. For touchpads,
libinput establishes the baseline when it recognizes a pinch. Touchscreens use
the initial geometry at the supported contact count.

These pinches do not pan or zoom the canvas. In the persistent switcher, click or
tap a preview, or press Enter, to activate it. Escape or a background click/tap
dismisses it. Three-finger navigation and scrolling cannot move the underlying
canvas while the switcher is open.

## Four- and five-finger window switching

Swipe four fingers horizontally on a native touchpad, or four/five on a
touchscreen, to select windows. Live windows move into the same grid used by
Alt+Tab. Continued swiping advances the selection; reversing moves back. The
original application keeps focus and the viewport stays fixed while held. Lift
to activate and reveal the selection, or press Escape to cancel.

Recognition starts after 32 logical units of horizontal movement, with horizontal
distance at least 1.5 times vertical distance. Further selections are 96 units
apart; reversing requires 48 units back across the last threshold. A predominantly
vertical movement does not switch windows for that sequence.

Touchscreens arbitrate between switching and pinching. An approximately 8% spread
change claims pinch recognition; qualifying horizontal translation claims a swipe.
If a frame satisfies both, pinch wins. Once claimed, the gesture keeps its meaning
until it ends.

An active switcher, input grab, window move, or resize prevents starting another
switching swipe. Keyboard input, pointer clicks, and scrolling do not reach
applications while the switcher is held. Cancellation restores the previous
focus and view; device removal, lock, and seat reset clear ownership so late events
cannot reopen it.

## Touchscreen contact changes

Place all intended fingers before moving. Contacts can arrive in separate frames,
and adding one changes the center and spread even if no finger has moved.

- Adding a fourth contact to three-finger navigation cancels its temporary canvas
  transform and starts switcher recognition from the four-contact geometry.
- Adding a fifth contact cancels the four-contact candidate or active swipe and
  starts a fresh five-contact baseline.
- A sixth contact suppresses the remaining sequence.
- The first lift ends the gesture. Remaining contacts cannot become a click,
  drag, or new gesture until every contact has lifted.

Contacts from separate devices never combine into one gesture. An active
touchscreen sequence also blocks touchpad gesture starts. Native touchscreen
coordinates map to the primary output; per-device monitor calibration is not part
of this mapping.

## Application touch and pointer fallback

A single contact on a touch-capable Wayland application is delivered as
`wl_touch`, with a stable target surface and local coordinate mapping. Shell clients
have dedicated touch routing for controls, scrolling, and compositor-owned actions.
The same sequence must not also generate a synthetic pointer click.

A client can request a move or resize using the active touch serial. The compositor
validates the seat, surface, serial, and current ownership, cancels the client's
touch sequence, and takes over window geometry. No synthetic mouse press or later
client touch-up accompanies that handoff. Cancellation and extra contacts cannot
turn the remainder into a click on another surface.

XWayland and clients without the direct-touch path retain pointer emulation.
There, a tap clicks on release; movement of at least eight logical units starts
a drag from the original contact position. Adding a contact before the drag
suppresses the click and allows later desktop gesture recognition. Adding one
after a drag starts releases that drag and suppresses the rest of the sequence.
Two-finger touchscreen input does not navigate the canvas.

Client disappearance, focus or geometry changes, device removal, and seat reset
can revoke touch ownership. A cancelled sequence must remain consumed until its
contacts drain. Keep this distinct from whether a new action is allowed to begin.

## Backend availability

Native touchpad gestures arrive through Smithay's libinput backend. Four-finger
switcher gestures are the supported native touchpad path; five-finger touchpad
recognition is not part of that contract. The compositor can accept five-finger
events from a backend that supplies them, which does not make such events
available from every touchpad or libinput configuration.

Touchscreens provide raw contacts, so Hearthspace recognizes both four- and
five-contact switcher gestures. The device must report the required simultaneous
contacts.

The pinned Smithay/winit adapter reports two-finger touchpad pinches and no
touchpad swipes. Its touchpad path therefore cannot drive the three-/four-finger
desktop gestures. Nested touchscreen input uses the same recognizer as native
input, with an adapter for winit's unframed physical positions. Shell controls and
keyboard shortcuts remain alternatives when a backend does not supply a gesture.

Desktop gesture recognition belongs to the compositor. It is separate from the
Wayland pointer-gestures extension used to forward gesture events to applications.

## Cancellation and interruption

Canvas gestures do not begin during a window operation or pointer/keyboard grab,
including popup grabs. Updates from another device or gesture type cannot change
the active gesture.

A normally completed three-finger gesture keeps its final view. Cancellation,
device removal, nested focus loss, and native session reset cancel an active
sequence; canvas cancellation restores its starting view. If libinput completed
a gesture normally before reporting removal, that completed view remains.

A newer mouse press, shell/scroll navigation, output-geometry change, or observed
grab instead interrupts navigation at the current view. Late events from that
sequence cannot restore an obsolete viewport or change window presentation states.
An unfinished switcher pinch is discarded without changing the switcher. Reset and
removal also discard successful ends queued in a frame but not yet dispatched.
Lock or VT loss dismisses an already-open switcher through its own reset path.

Gesture math uses absolute scale and total translation from the starting
viewport. Touchpad deltas accumulate as floating-point values without another
output-scale division; touchscreen translation is the current center minus its
initial center. Round the resulting canvas offset once. Re-hit-test the pointer
after a viewport change, since a window can move under a stationary pointer.

## Verification boundaries

Deterministic regressions cover transform math, thresholds, device isolation,
contact batching, promotion between contact counts, and cancellation. Input
integration checks also need to exercise direct client touch, authenticated
move/resize handoff, pointer fallback, and release ownership.

For the kernel-to-compositor path, the native acceptance tools create temporary
virtual touchpad and touchscreen devices. `scripts/pan-smoke` exercises translation
and combined scaling; `scripts/pinch-smoke` exercises zoom and contact arbitration;
`scripts/window-switcher-smoke` exercises held switching, pinches, and cancellation.
See [development](DEVELOPMENT.md) and [native testing](NATIVE_TESTING.md) for setup,
commands, and persistent-switcher acceptance.

Synthetic device tests can establish routing and visible presentation in the
chosen environment. They do not reproduce a person's physical finger feel or
establish gesture support for every input device.
