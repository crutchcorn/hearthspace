# Canvas Gestures

Three fingers navigate the canvas. Four fingers on native touchpads, or four/five
on touchscreens, pinch to open or dismiss a
[temporary overview of live windows](./WINDOW_MANAGEMENT.md#canvas-overview), or
swipe left/right to [switch windows](./WINDOW_MANAGEMENT.md#held-window-switcher).
These gestures work without modifiers and do not change application content.
Five-finger touchpad gestures are unavailable through the native libinput backend;
see [backend availability](#backend-availability).

## Three-Finger Pan and Zoom

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

## Four- and Five-Finger Overview

Place four fingers on a native touchpad, or four/five on a touchscreen, bring
them together, and lift to open overview. Spread the same supported number of
fingers and lift to dismiss it. Inward means a final spread of at most 80% of the
starting spread; outward means at least 125%. For touchpads, libinput establishes
that baseline after enough motion to recognize a pinch. Touchscreens use the
initial geometry at the supported contact count. Only the final spread on a
normal gesture end commits the action. Crossing a threshold and returning to the middle before lifting does nothing,
as does a cancelled gesture. At most one transition occurs per completed pinch.

These pinches do not pan or zoom the canvas. Parallel motion and rotation do not
change the overview state. Four-finger touchpad pinches and four-/five-finger
touchscreen pinches control overview; three remain reserved for canvas navigation.
Horizontal swipes switch windows outside overview. While overview
is open, three-finger navigation and scrolling cannot move the underlying canvas.

Touchscreen contacts often arrive in separate frames. Adding a fourth contact
to an active three-finger gesture cancels that temporary canvas transformation,
restores its original viewport and maximization, and starts overview recognition
from the four-contact geometry. Adding a fifth contact cancels the four-contact
candidate and starts a fresh five-contact baseline, so placing another finger
cannot itself count as expansion. Place all intended fingers before pinching.
Adding a sixth contact cancels the gesture and suppresses the rest of the touch
sequence. After the first finger lifts, the remaining contacts cannot become
clicks, drags, or a new gesture until all fingers lift.

Use the **Overview** toolbar button or Super+O when a supported pinch is
unavailable. Inside overview, arrows or Tab/Shift+Tab select a preview,
Enter activates it, and Escape dismisses it. Clicking or tapping a preview also
activates it. See [window management](./WINDOW_MANAGEMENT.md#canvas-overview)
for the complete controls and preserved window state.

## Four- and Five-Finger Window Switching

Slide four fingers left/right on a native touchpad, or four/five on a
touchscreen, to select the next/previous window. A compact strip of live previews
stays open until the gesture ends. Each selection focuses its window and pans
the canvas to it with a quick 180 ms eased animation. Continued swiping steps
through more windows; reversing direction moves back. Lift to keep the selected
window, or press Escape to cancel and restore the previous focus and view.

Recognition starts after 32 logical pixels of horizontal movement, with
horizontal distance at least 1.5 times vertical distance. Further selections are
96 pixels apart; reversal requires 48 pixels back across the last selection
threshold. This prevents small tremors from changing the selected window.
A predominantly vertical movement claims no switching action for that sequence.

Touchscreens distinguish swipes from overview pinches using both center movement
and spread. An approximately 8% spread change claims pinch recognition; a
horizontal movement meeting the swipe threshold claims switching. Once claimed,
the gesture keeps its meaning until it ends. Place all intended fingers before
moving: adding a fifth finger rebases the candidate and cancels a switch already
in progress. Remaining contacts after the first lift cannot start another action.

An active overview, input grab, move, resize, or keyboard-owned switcher prevents
starting a swipe. A cancelled swipe restores the previous focus and view. Device
removal, lock, or VT loss clears the interaction, so late events cannot reopen it.
Keyboard input, pointer clicks, and scrolling do not reach applications while
the switcher is held.

## Backend Availability

Native touchpad gestures come from libinput through Smithay. The installed
libinput 1.31.1 backend only processes gestures with at most four active contacts;
its [gesture state processing](https://gitlab.freedesktop.org/libinput/libinput/-/blob/1.31.1/src/evdev-mt-touchpad-gestures.c)
explicitly excludes five. Hearthspace therefore supports four-finger overview
pinches and window-switching swipes on this native touchpad path. Its compositor
policy can accept five-finger gestures from a backend that supplies them, but this does not enable
five-finger touchpad recognition in libinput. Touchscreens supply raw contacts,
so Hearthspace can recognize both four- and five-finger pinches and swipes there.

The pinned Smithay/winit backend only delivers two-finger touchpad pinch events
and no touchpad swipe events, so three-finger navigation and four-finger
overview pinches on touchpads require the native backend. Nested touchscreen
events use the same recognizer, with an adapter for winit's unframed physical
coordinates. Nested X11 does not supply touchpad pinch events.

## Interaction and Cancellation

A canvas gesture does not begin during a window drag, resize, pointer or keyboard
grab (including popup grabs). An active touchscreen sequence blocks touchpad
gestures. Contacts from separate touchscreens are never combined. Updates from
another device or gesture type cannot change the active gesture.
Touchscreen positions use the compositor's existing primary-output mapping.

Touchscreen taps click on release. A single contact moving at least eight logical
pixels starts a drag from its original position. Adding another contact before
that point suppresses the click and allows a later three-finger canvas gesture
or four-/five-finger overview gesture. Adding contacts after a drag has started
releases the drag and suppresses the rest of that touch sequence. Two-finger
touchscreen gestures do not pan, zoom or click.

The first normal lift ends three-finger touchscreen navigation at its current
view. A fourth contact cancels that navigation and can promote the sequence to
overview; touch cancellation cancels it without promotion. Remaining contacts
after a lift cannot start another gesture or become a click until all have lifted.

Ending a three-finger gesture normally retains the current view. Cancelling it,
removing its device, losing nested-window focus, or suspending the native session
restores the view from that gesture's beginning and re-maximizes windows restored
by that gesture. Libinput may end a gesture normally before reporting device removal;
that completed gesture retains its final view.
A mouse press, shell/scroll navigation, output geometry change, or newly observed
grab instead ends the gesture at its current view, letting the newer interaction
proceed. Later events from the interrupted gesture cannot restore an obsolete
viewport or re-maximize those windows.

An unfinished overview pinch is discarded on cancellation, device removal,
interruption, nested focus loss, or native VT/suspend input reset. It does not
change overview or the canvas. Reset and removal also cancel a successful touch
end that was queued earlier in the same frame but has not yet been dispatched.
Locking or losing the active VT dismisses any already-open overview separately.

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
completion, and cancellation. Overview regressions additionally cover exact
contact counts, inward/outward thresholds, final-scale reversal, three-to-four
contact promotion, four-to-five contact rebasing, queued-end cancellation, and
suppression after the first lift. For the kernel-to-compositor path and visible
native presentation, use the optional
[native gesture acceptance tests](../CONTRIBUTING.md#optional-native-pinch-testing).
They create temporary virtual touchpad and touchscreen devices and exercise the
normal libinput path. `scripts/pan-smoke` checks visible translation and combined
scaling; `scripts/pinch-smoke` checks zoom, contact arbitration and device removal.
`scripts/overview-smoke` checks four-finger touchpad and four-/five-finger
touchscreen overview gestures, recording the five-finger libinput limit separately.
These tests validate the input and rendering pipeline, without emulating a
person's physical finger feel on the hardware.
