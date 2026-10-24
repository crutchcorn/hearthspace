# Audio and brightness

Use the sun button in the system tray to open **Brightness**, or the speaker
button to open **Sound**. Each dropdown shows its own slider and percentage.
Sound also shows the current audio output and a mute/unmute button.
Press Escape, activate the same tray button, or click outside to close a dropdown.
Opening another shell popup closes it.

Drag either slider with a pointer or touchscreen. Keyboard navigation shows a
complete outline inside the slider. Hover never adds an outline; clicking or
dragging hides it. Tab moves between controls and arrow keys change a focused
slider by one percentage point (Shift changes ten). Home and End select the limits.
Brightness ranges from 1–100%, keeping the backlight on. Volume ranges from
0–100%, without amplification; a positive volume adjustment also unmutes.
Volume changes preserve the current channel balance and silent channels. When
all channels are already zero, increasing volume starts them at equal levels.

The keyboard's brightness and volume keys adjust by five percentage points.
Holding a level key repeats after a short delay; mute toggles once per press.
Agent input accepts `brightness-up`, `brightness-down`, `volume-up`,
`volume-down`, and `volume-mute` using the same input path.

Confirmed hardware-key adjustments show a bottom-center on-screen display (OSD)
for about 1.6 seconds. Further hardware-key adjustments replace its value and
restart the timeout. Dropdown sliders and mute update their popup only,
including when operated with the keyboard. They never open, change or extend an
existing OSD; any adjustment error is shown in the corresponding dropdown.
The OSD shows the actual device reading or mute state; failures show an
unavailable explanation. It neither takes focus nor intercepts pointer/touch
input, and remains visible above fullscreen apps and window previews.
Locking or switching away clears pending keys and visible feedback.

## Optional hardware and services

Brightness uses the kernel backlight interface and logind's `SetBrightness`
method for the foreground native session. It does not require root privileges,
new device permissions, or a separate brightness daemon. Without a usable
backlight, the slider is disabled with an explanation; there is no fabricated
zero reading. When several kernel backlights exist, the first usable firmware,
platform, then raw device is chosen deterministically. This controls that system
backlight from any output's Brightness dropdown. External-monitor DDC/CI controls are
not implemented, and nested/headless sessions do not change host brightness.

Volume uses the existing PulseAudio-compatible service, including PipeWire Pulse,
through `pactl` 16 or newer. It honors the session's existing audio routing,
including `PULSE_SERVER`. It follows the current default output and reconnects
after service or device changes. No battery is required. The optional utility
and setup are documented in [dependencies](DEPENDENCIES.md).

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
real shell and compositor with a private fake audio endpoint, then exercises the
individual Sound and Brightness dropdowns, pointer/keyboard sliders, media keys,
mute, OSD dismissal, preserved application input, unavailable devices and recovery.
Evidence is saved under `audio-and-brightness` within the configured shell evidence
directory. It never changes host audio or
brightness. Compositor E2E separately checks OSD placement, pointer/touch
passthrough, fullscreen/preview layering, lock and event expiry.

Native acceptance uses the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
on an owned VT. Save and restore the exact original backlight value, per-channel
audio volumes and mute state, and verify device readings alongside PNG/metadata
and accessibility assertions. Keep these host checks separate from headless CI.
