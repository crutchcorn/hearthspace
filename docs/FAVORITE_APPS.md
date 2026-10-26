# App gestures and favorites

Open **Settings → Gestures** to assign a drawn shortcut to an installed
app. Choose **Add gesture**, pick an app, draw one continuous shape, then save
it. **Back to gestures** cancels the recording without saving. Existing assignments offer
**Change gesture** and **Remove gesture**. Removing a gesture keeps the app pinned
in the dock; use the desktop app's **Remove from favorites** action to unpin it.

Draw by clicking and dragging with a mouse or one finger on a trackpad, or by
drawing directly with one finger on a touchscreen. You can redraw before saving.
Shape previews help you remember assignments. Each shortcut uses its desktop-file
ID and the existing safe app launcher, never a saved shell command.

Open Apps List with Meta, its dock button, or a one-finger upward touchscreen
swipe starting on the bottom-right dock. Short taps still activate dock buttons;
horizontal/downward swipes and multi-contact interruptions do not open Apps List.
The **Draw** tab opens the gesture pad. Its Settings icon opens the Settings
app directly on its Gestures page. An empty query in the **Apps** tab lists all
installed apps alphabetically; favorites stay in the dock even when closed.
Typing searches applications and open windows as usual.

Release the mouse button or lift your finger after drawing: a clear match launches
the app immediately. An unrecognized or ambiguous shape keeps search open and
shows feedback. Select the **Apps** tab to leave drawing mode. One touchscreen
finger scrolls Settings and search results outside the drawing area; a drag on
the drawing area stays a drawing.

The gesture matches a saved shape, not the meaning of a letter or an app name.
It works independently of the optional AI setting used for window gathering.
Recognition is deterministic: it resamples the stroke uniformly, removes
position and uniform scale, and allows modest rotation, drawing in reverse, and
different starting points on closed shapes. It rejects very small strokes,
confusable assignments, uncertain matches, and invalid or excessive input.
Distinct shapes work better than small variations of one shape. There is no
training service, random classification, or network access.

Assignments are saved atomically in
`$XDG_CONFIG_HOME/hearthspace/favorites.json` (normally
`~/.config/hearthspace/favorites.json`). Settings reports malformed files and
write errors instead of silently replacing the configuration. Search reloads
changes automatically. Removed applications remain visible as unavailable
assignments so they can be removed in Settings. The existing favorites file format
is retained: saving a gesture preserves other favorites and their order, including
apps pinned without a gesture. Settings lists gesture assignments; the dock lists
installed favorites with or without gestures.

Writers serialize the disk comparison and atomic rename with the persistent
`favorites.json.lock` sibling. The lock is private and released when its writer
exits; the file remains so concurrent processes always lock the same inode.
A save waits at most 500 ms for another writer, then reports a retryable busy
error. A stale editor must reload before saving; a failed save preserves both
its local assignments and the other process's committed file.

## Input and automation

Hearthspace UI and other Wayland applications that bind touch receive native
touch events. X11 applications and Wayland clients without touch support use
pointer emulation. Scroll gestures cancel
a pending button tap, while drawing controls retain their own pointer capture.

The shared Xilem drawing widget records candidate shapes in Settings. Recording
never launches an app; saving is explicit. Search drawing uses compositor-owned
input scoped to the focused, mapped search window and its drawing area. Only
primary-button dragging and single-contact touch are captured. Canvas swipe and
pinch handlers retain their existing behavior.

Ordered compositor events carry a drawing generation, stroke ID and sequence.
The shell requires a complete sequence and a fresh, single-use completion token
before a match can launch anything. The final command validates the token and
launches the catalog app in the same compositor operation. Leaving the area, a second contact,
focus/session loss, or a layout change cancels the stroke. Layout changes require
a fresh drawing generation. Input remains consumed until the captured contact or
button is released, preventing a cancelled drawing from becoming a canvas drag.

The accessible drawing area is named **Gesture drawing area**. Its status and
point count are exposed to accessibility clients. Compositor `status` includes
`favorite_gesture` with the armed generation, output, local bounds and active
stroke ID. This state is redacted with other protected session state while locked.
The private arm/disarm/accept commands are shell transport, not shortcuts for
bypassing physical input validation.

The headless acceptance scenario exercises Settings configuration and search
launching with real pointer events. Native validation additionally exercises the
libinput touchscreen path using the owned VT harness. See
[NATIVE_TESTING.md](NATIVE_TESTING.md) for session ownership and cleanup.

## Regression checks

Run the complete workspace checks from the [development guide](DEVELOPMENT.md).
The focused Python acceptance test is:

```sh
HEARTHSPACE_E2E=1 HEARTHSPACE_REQUIRE_ATSPI=1 \
python3 -m unittest discover -s scripts/tests -p test_favorite_ui_headless.py -v
```

Build all workspace binaries with `udev,e2e,test-apps` first and use the
headless rendering setup in [E2E testing](E2E_TESTING.md). The test creates
private application and configuration directories, records through the real
Settings drawing widget, reads back the saved shape, and launches through the
real search input path. It also verifies unknown-shape rejection, leaving the
area, closing drawing mode, and Escape while holding a stroke.

Background window-list or desktop-catalog errors remain visible in browsing mode,
but do not change the active drawing layout or its window size. Gesture feedback
and app-launch failures remain visible. This keeps a background query timeout
from moving the pad and invalidating a stroke already in progress. Layout
regressions check the measured pad and content bounds alongside that input
ownership contract.
