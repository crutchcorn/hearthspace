# Native TTY keyboard switching

`Ctrl+Alt+F1` through `Ctrl+Alt+F12` select the corresponding Linux virtual
terminal from native Hearthspace. Either Ctrl and Alt key works. These physical
keyboard shortcuts precede client grabs, search, the window switcher and the
lock screen. They preserve the running session and do not unlock it.
Media-key-first keyboards may also require Fn. Nested and headless sessions do
not switch host VTs.

The native backend uses Smithay's session `change_vt` API. Per-device key tracking
suppresses the intercepted function key's repeat and release, even if switching
fails or the modifiers are released first. Pausing clears queued search input and
held modifiers before the native input source is recreated.

## Validation

Follow the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and [native harness guide](NATIVE_TESTING.md). Reserve an otherwise unused text
console before injecting a return chord; never type into another desktop or an
unowned login session. Control-socket key injection intentionally does not switch
the host VT. Native automation must use an owned uinput keyboard through libinput.

Check three round trips: a focused application, an open window switcher, and a
locked session. Record the test VT, reserved return console and original VT
before switching. Inspect the kernel's active VT with:

```sh
cat /sys/class/tty/tty0/active
```

On switching away, require compositor inactivity. On an unlocked return, require
`active` and `recovery_ready`, the same application identity, a fresh page-flip
capture and exact text received by that application. The switcher must close
during input reset. On a locked return, require the lock to remain active, a new
confirmed shield presentation, redacted window information and denied capture.
Only local authentication may unlock the real locker.

Stop through the owning harness and verify original-VT restoration, stopped
scopes, runtime removal and removal of the injected keyboard. Release reserved
consoles only after leaving them. Retain manifests, binary provenance, traces,
assertions and unlocked PNG/metadata pairs with each run. A uinput pass does not
establish a particular keyboard's Fn behavior; test physical keys separately.
Environment coverage belongs in [tested environments](TESTED_ENVIRONMENTS.md).

## Presentation recovery

A VT switch can succeed while return readiness fails. Inspect compositor status
and the native presentation logs before attributing that failure to the shortcut.
A modeset completion with both timestamp and sequence zero is not evidence that
a current frame was presented.

Hearthspace retires that untimed buffer once per surface, discards its staged
capture and lock marker, and requests another frame. Only a current, nonzero
timestamp establishes presentation or recovery readiness. A short grace period
allows the follow-up frame without an immediate surface rebuild; the five-second
recovery budget remains absolute. The implementation and regression cases live
in [`udev/event_cutoff.rs`](../crates/hearthspace-compositor/src/compositor/udev/event_cutoff.rs).

Keyboard regressions cover all twelve function keys, both modifier sides,
multiple keyboards, repeats, releases, failed/no-op switches and device removal.
Headless recovery tests cover pending search input and held modifiers; use the
[E2E renderer environment](E2E_TESTING.md#headless-compositor) for those checks.
They complement a real VT round trip rather than establishing its presentation
or recovery result.
