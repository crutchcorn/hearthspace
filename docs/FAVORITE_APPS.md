# Favorite apps and drawn shortcuts

Open **Settings → Favorites** to add installed apps and assign gestures. Each
favorite uses its desktop-file ID, so a saved shortcut does not contain a shell
command. Favorites without gestures can still be launched from search.

Choose **Assign gesture**, draw one continuous shape in the drawing area, then
save it. Draw by clicking and dragging with a mouse or one finger on a trackpad,
or by drawing directly with one finger on a touchscreen. You can redraw before
saving. Shape previews help you remember assignments; changing or removing an
assignment happens in Settings too.

Open search with Meta or the search icon. Your favorites appear when the query
is empty. Choose **Draw to launch** to activate its drawing area. Release the
mouse button or lift your finger after drawing: a clear match launches the app
immediately. An unrecognized or ambiguous shape keeps search open and shows
feedback. **Back to search** leaves drawing mode. Ordinary touchpad navigation
outside the drawing area is unchanged.

Recognition is deterministic. It resamples the stroke uniformly, removes
position and uniform scale, and allows modest rotation, drawing in reverse, and
different starting points on closed shapes. It rejects very small strokes,
confusable assignments, uncertain matches, and invalid or excessive input.
Distinct shapes work better than small variations of one shape. There is no
training service, random classification, or network access.

Favorites are saved atomically in
`$XDG_CONFIG_HOME/hearthspace/favorites.json` (normally
`~/.config/hearthspace/favorites.json`). Settings reports malformed files and
write errors instead of silently replacing the configuration. Search reloads
changes automatically. Removed applications remain visible as unavailable
favorites so they can be removed in Settings.

## Input and automation

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

## Validation record — 2026-10-15

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
