# Native TTY keyboard switching

`Ctrl+Alt+F1` through `Ctrl+Alt+F12` select the corresponding Linux virtual
terminal from native Hearthspace. Either Ctrl and Alt key works. These physical
keyboard shortcuts precede client grabs, search, the window switcher, and the
lock screen.
They preserve the running session and do not unlock it. Media-key-first laptop
keyboards may also require Fn. Nested and headless sessions do not switch host VTs.

The native backend uses Smithay's session `change_vt` API. Per-device key tracking
suppresses the intercepted function key's repeat and release, even if switching
fails or the modifiers are released first. Pausing clears queued search input and
held modifiers before the native input source is recreated.

## Validation

Follow the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and [native harness guide](NATIVE_TESTING.md). Reserve an otherwise unused text
console before injecting a return chord; never type into another desktop or an
unowned login session. Control-socket key injection intentionally does not switch
the host VT. Native automation must use an owned uinput keyboard through libinput.

Check three round trips: a focused application, an open window switcher, and a
locked session. Observe the active kernel VT and compositor inactivity after switching
away. On return, require `active` and `recovery_ready`, preserved application
identity, a fresh page-flip capture, and exact text received by the application.
The switcher should close during input reset. A locked session must remain locked,
confirm its new shield presentation, redact window information, and deny capture.
Stop through the owning harness and verify its original VT, scopes, and runtime.

### Hardware acceptance — 2026-10-15

Acceptance passed on the physical GPD laptop, AMD Strix Radeon 880M/890M
(`1002:150e`, `amdgpu`), Linux `7.0.0-38-generic`, and eDP-1 at 2560×1600, scale 1.
An isolated native session occupied tty7 and returned to the recorded tty3.

`/tmp/hearthspace-vt-fixed.DxhO5i` retains manifests, binary hashes, screenshots,
capture metadata, libinput traces, and assertions. Left Ctrl/Alt selected tty12
from the desktop and lock screen. The kernel return chord selected tty7 from the
reserved text console. The same Foot window received exactly `vtok\n` after the
unlocked return. Captures confirmed fresh page flips. The locked return retained
the lock, confirmed its presentation, and refused capture. This tests native input through
uinput; it does not establish a particular laptop keyboard's Fn behavior.

Cleanup restored tty3, stopped the exact native unit with PID 0 and no cgroup,
removed its app scopes, runtime, and injected keyboards, and released both reserved
consoles after returning to tty3. The agent remained reachable throughout. No
other native harness or GPU-heavy test ran concurrently.

### Recovery regression found by the round trip

Initial attempts in `/tmp/hearthspace-vt-native.sRJ5Zk` and
`/tmp/hearthspace-vt-debug.eMaHYa` switched correctly but failed return readiness.
`/tmp/hearthspace-vt-trace.leZrF5` identified an all-zero timestamp and sequence on
the first modeset completion. The old timestamp cutoff rejected it as stale,
repeatedly rebuilt the output, and never acknowledged a new frame. All failed
attempts completed owned-session cleanup and restored tty3.

This matches the AMD full-modeset behavior documented in
[Mesa's upstream fix](https://chromium.googlesource.com/external/gitlab.freedesktop.org/mesa/mesa/+/b4a1756fd1f37b28858ba4c871188c1bf2b6f4f3).
Hearthspace now retires that untimed buffer once per surface, discards its staged
capture and lock marker, and requests another frame. Only a current, nonzero
timestamp establishes presentation or recovery readiness. A short grace period
allows this follow-up frame to arrive without an immediate surface rebuild;
the original five-second recovery budget remains absolute.

Unit regressions cover all twelve function keys, both modifier sides, multiple
keyboards, repeats, releases, failed/no-op switch lifetimes, device removal, stale
events, the one-shot untimed retry, sequence wraparound, and bounded recovery.
Headless integration also verifies pending search keys cannot leave Ctrl/Alt held
across session recovery. Full-check logs are in
`/tmp/hearthspace-vt-checks.Fk8lIQ`; software headless checks use llvmpipe and
lavapipe as documented in the E2E guide.

Final validation passed 854 Rust tests with E2E and required accessibility/tray
coverage, 388 Python tests with E2E, the workspace/all-target build using
`udev,e2e,test-apps`, rustfmt, strict Clippy, Ruff lint/format, and diff checks.
