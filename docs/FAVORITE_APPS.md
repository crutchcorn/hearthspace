# Search gestures

Open **Settings → Search → Gestures** to assign a drawn shortcut to an installed
app. Choose **Add gesture for …**, draw one continuous shape, then save it.
Cancelling a recording does not add anything. Existing assignments offer
**Change gesture** and **Remove gesture**; there is no separate favorites list.

Draw by clicking and dragging with a mouse or one finger on a trackpad, or by
drawing directly with one finger on a touchscreen. You can redraw before saving.
Shape previews help you remember assignments. Each shortcut uses its desktop-file
ID and the existing safe app launcher, never a saved shell command.

Open search with Meta, the search icon, or a one-finger downward touchscreen
swipe starting on the top bar. Short taps still activate toolbar buttons;
horizontal/upward swipes and multi-contact interruptions do not open search.
The drawing icon at the right of the search input enables **Draw to launch**.
The adjacent Settings icon opens the Settings app. Empty search does not list
assigned apps. Typing searches applications and open windows as usual.

Release the mouse button or lift your finger after drawing: a clear match launches
the app immediately. An unrecognized or ambiguous shape keeps search open and
shows feedback. **Back to search** or pressing the drawing icon again leaves
drawing mode. One touchscreen finger scrolls Settings and search results outside
the drawing area; a drag on the drawing area stays a drawing.

Recognition is deterministic. It resamples the stroke uniformly, removes
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
is retained for compatibility: saved shapes are preserved, while old entries
without a gesture are hidden and retired on the next successful assignment save.

## Input and automation

Hearthspace UI uses standard Wayland touch events for direct manipulation; other
applications retain the existing pointer-emulation behavior. Scroll gestures cancel
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

Run the complete workspace checks from [CONTRIBUTING.md](../CONTRIBUTING.md).
The focused Python acceptance test is:

```sh
HEARTHSPACE_E2E=1 HEARTHSPACE_REQUIRE_ATSPI=1 \
LIBGL_ALWAYS_SOFTWARE=1 \
VK_DRIVER_FILES=/usr/share/vulkan/icd.d/lvp_icd.json \
python3 -m unittest discover -s scripts/tests -p test_favorite_ui_headless.py -v
```

Build all workspace binaries with `udev,e2e,test-apps` first. The Vulkan override
is for Linux systems with Mesa's Lavapipe ICD installed at that path; it keeps
headless GL and Vulkan rendering on the same software device. The test creates
private application and configuration directories, records through the real
Settings drawing widget, reads back the saved shape, and launches through the
real search input path. It also verifies unknown-shape rejection, leaving the
area, closing drawing mode, and Escape while holding a stroke.

## Initial implementation validation — 2026-10-15

- Full format, Python lint, strict workspace Clippy and all-target build passed.
- All 808 Rust tests and all 385 Python tests passed with strict headless E2E
  enabled. The Settings-to-search scenario and screenshots used 2560×1600 at 2×.
- The native scenario passed on physical AMD Strix Radeon 880M/890M hardware,
  `amdgpu`, Linux `7.0.0-38-generic`, with eDP-1 at 2560×1600 and 1× scale.
  It recorded and saved the shape through Settings, observed immediate launches
  from shifted/scaled pointer and touchscreen strokes, rejected an unknown shape,
  cancelled an out-of-area stroke and a held stroke on Escape, and cancelled a
  second touchscreen contact. Temporary uinput touchscreen events traversed
  libinput; pointer injection exercised the same primary-button path used by
  mouse and trackpad click-drag.
- Native captures completed page flips. The owned session used TTY7 and returned
  to its recorded TTY2; cleanup reported stopped session/application scopes and
  removed runtime. A later observation found TTY3 active; that pre-existing user
  session was left untouched.
- Local evidence: `/tmp/hearthspace-favorites-checks.OUgld6/` and
  `/tmp/hearthspace-favorites-native.Q2NXmz/` (logs, results, screenshots with
  metadata, libinput trace and cleanup manifest).

The first full Rust run caught an obsolete empty-search size assertion. Its
replacement still verifies compact content, growth for results and shrinkage on
clearing the query, while accounting for the new favorites controls. Subsequent
full checks passed. No operating-system configuration was changed.

## Search and touch navigation validation — 2026-10-15

The follow-up removes the favorites list, adds inline drawing and Settings
actions, and makes gesture assignment a single operation in Settings. Regression
coverage includes atomic saves, rejected/conflicting assignments, legacy entries,
keyboard navigation, and touch scrolling without accidental button activation.
Shared scrolling tests also cover pointer capture in drawing controls and editors,
touch cancellation, scroll limits, and logical coordinates at 2× scale.

Full formatting, Python lint, strict workspace Clippy and the all-target build
passed. The final complete runs passed all 831 Rust tests and all 385 Python tests
with strict headless E2E enabled. The first Python run caught a stale empty-search
hint in the compact-popup acceptance check; the updated check and full rerun
passed. Headless gesture configuration and launch captures used 2560×1600 at 2×.

Native checks passed on the same AMD hardware and eDP-1 output described above:

- A downward touchscreen swipe from the top bar opened search; repeating it kept
  search open. A horizontal swipe left it closed, and a short tap still activated
  the search button. These gestures left the canvas position and zoom unchanged.
- One-finger Settings scrolling moved the app list without triggering the touched
  app's action or moving the window or canvas.
- Settings recorded a touchscreen shape only on release. A second contact
  cancelled recording, and cancelling the draft left the saved assignment file
  unchanged.
- Search launched the assigned app immediately from translated/scaled pointer
  and touchscreen strokes, with unknown-shape and cancellation checks passing.

The owned TTY7 session returned to its recorded original TTY3. Cleanup verified
that its session/application scopes stopped and its runtime was removed.
Native evidence is in `/tmp/hearthspace-search-native.HVX2ZS/`; automated check
logs and headless evidence are in `/tmp/hearthspace-search-checks.iULxsN/`.
No operating-system configuration was changed.
