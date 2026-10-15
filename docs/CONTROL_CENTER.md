# Control center

Use the sliders icon beside the power button to open **Control center**. The
popup contains brightness and volume sliders, percentages, the current audio
output, and a mute/unmute button. It closes with Escape, its close button, or a
click outside. Opening another shell popup closes it.

Drag either slider with a pointer or touchscreen. Keyboard focus is visible;
Tab moves between controls and arrow keys change a focused slider by one
percentage point (Shift changes ten). Home and End select the limits.
Brightness ranges from 1–100%, keeping the backlight on. Volume ranges from
0–100%, without amplification; a positive volume adjustment also unmutes.
Volume changes preserve the current channel balance and silent channels. When
all channels are already zero, increasing volume starts them at equal levels.

The keyboard's brightness and volume keys adjust by five percentage points.
Holding a level key repeats after a short delay; mute toggles once per press.
Agent input accepts `brightness-up`, `brightness-down`, `volume-up`,
`volume-down`, and `volume-mute` using the same input path.

Confirmed adjustments show a bottom-center on-screen display (OSD) for about
1.6 seconds. Further adjustments replace its value and restart the timeout.
The OSD shows the actual device reading or mute state; failures show an
unavailable explanation. It neither takes focus nor intercepts pointer/touch
input, and remains visible above fullscreen apps and the canvas overview.
Locking or switching away clears pending keys and visible feedback.

## Optional hardware and services

Brightness uses the kernel backlight interface and logind's `SetBrightness`
method for the foreground native session. It does not require root privileges,
new device permissions, or a separate brightness daemon. Without a usable
backlight, the slider is disabled with an explanation; there is no fabricated
zero reading. When several kernel backlights exist, the first usable firmware,
platform, then raw device is chosen deterministically. This controls that system
backlight from any output's control center. External-monitor DDC/CI controls are
not implemented, and nested/headless sessions do not change host brightness.

Volume uses the existing PulseAudio-compatible service, including PipeWire Pulse,
through `pactl` 16 or newer. It honors the session's existing audio routing,
including `PULSE_SERVER`. It follows the current default output and reconnects
after service or device changes. No battery is required. The optional utility
and setup are documented in [CONTRIBUTING](../CONTRIBUTING.md#optional-desktop-dependencies).

Hardware work runs outside the UI thread. Status polling yields to adjustments,
slider requests coalesce, and requests expire instead of replaying later.
Relative adjustments derive their level from the same device they will change,
so a device switch cannot transfer the previous output’s level to its replacement.
Each mutation rechecks that this desktop is active, unlocked and recovered;
media-key requests also require current single-use compositor authorization.
An uncertain write is reported and is never automatically retried.

The integrations follow the [kernel backlight ABI](https://docs.kernel.org/gpu/backlight.html),
[logind session API](https://github.com/systemd/systemd/blob/main/man/org.freedesktop.login1.xml),
and [PulseAudio volume UI guidance](https://www.freedesktop.org/wiki/Software/PulseAudio/Documentation/Developer/Clients/WritingVolumeControlUIs/).
These device controls are separate from Wayland output power or gamma protocols.

## Validation

Backend tests use private D-Bus/sysfs fixtures and fake audio processes to cover
missing devices, malformed readings, daemon/default-output changes, timeouts,
permission loss, cancellation and bounded output. UI tests exercise actual
Masonry sliders, accessibility values, keyboard focus, unavailable controls,
OSD lifetime, and stale-result handling.

The strict Python E2E suite includes `test_control_ui_headless.py`. It runs the
real shell and compositor with a private fake audio endpoint, then exercises
pointer/keyboard sliders, media keys, mute, OSD dismissal, preserved application
input, unavailable devices and recovery. It never changes host audio or
brightness. Compositor E2E separately checks OSD placement, pointer/touch
passthrough, fullscreen/overview layering, lock and event expiry.

Native acceptance uses the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
on an owned VT. Save and restore the exact original backlight value, per-channel
audio volumes and mute state, and verify device readings alongside PNG/metadata
and accessibility assertions. Keep these host checks separate from headless CI.

### 2026-10-15 acceptance

Formatting, Ruff, workspace Clippy with warnings denied, and the full native/E2E
workspace build passed. Ordinary Rust tests passed 672 tests; the strict
E2E-enabled Rust run passed 717. Python passed all 381 tests with E2E, required
AT-SPI, and the real tray-library fixture enabled (the ordinary run skips its
11 opt-in integration tests). Logs are retained in
`/tmp/hearthspace-control-release-checks/`, with real-shell control-center screenshots
and assertions in `evidence/shell/control-center/artifacts/`. Final validation
used `CARGO_INCREMENTAL=0`. Dependency advisory, ban and source checks also passed
with the repository’s CI settings.

An initial native attempt, `/tmp/hearthspace-controls-native-1`, stopped before
any control adjustments because the acceptance fixture compared PipeWire's
volatile sink index across a VT switch. Its owned session cleaned up completely,
and separate read-only verification confirmed the original raw brightness,
per-channel volume and mute state were unchanged. The subsequent fixture
compares stable physical audio properties, endpoint, sink name, active port and
channel map; a recreated sink index is retained only as diagnostic data.

That attempt's screenshot also exposed a fractional-height scrollbar over the
percentage labels. Popup content now rounds its intrinsic height upward before
measurement, keeping Portal's scroll extent consistent with the rounded window
size. A regression covers 1×, 1.25×, 1.5×, 2× and 3× scales, unobscured percentage
labels, and continued scrolling when content exceeds the height cap.

The final native run, `/tmp/hearthspace-controls-native-2`, passed using the
installed optimized release `c17ed98850ba` on the GPD G1619-05 (bare metal), AMD
Radeon 880M/890M with `amdgpu`, kernel `7.0.0-38-generic`, and eDP-1 at 2560×1600,
2× scale. Installed runtime hashes and root ownership matched the built release;
verification is in `installed-release.json` under the final check directory.

Pointer drags and focused arrow keys changed the real backlight and audio output.
Brightness key steps were verified at 27→32→27%, volume at 26→31→26%, and both
mute transitions were checked against the audio service. PNGs with page-flip
metadata and private accessibility snapshots confirm the panel and both OSDs;
the panel has no stray scrollbar. The terminal received an exact marker while
an OSD was visible, retained focus, and every key OSD dismissed automatically.

Cleanup restored the original backlight value (38341/63479), both original raw
audio channel levels (24903 each), and unmuted state. The native runtime and all
owned services stopped, the original GNOME session on TTY2 returned, and T3's
background service identities were preserved. An independent read-only check
after return confirmed these results in `artifacts/post-return-verification.json`.
The runner, restoration guard, input assertions, original state and results are
retained with the run's artifacts.

This acceptance injected input through the compositor's normal key/pointer path;
it did not press the physical Fn keys, inject touchscreen contacts, or play an
audible test sound. Asymmetric channel balance, unplug/replacement races and
missing hardware/services are covered by the isolated regressions and shell E2E.
