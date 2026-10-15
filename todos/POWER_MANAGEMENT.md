# Power management follow-up

The implemented lid/suspend baseline and its hardware evidence are in
[power handling](../docs/POWER.md) and [power acceptance](../docs/POWER_TESTING.md).
Physical lid closure on AC has triggered Hearthspace-owned suspend and protected
native recovery on the GPD host. This does not establish all-day laptop behavior.

## Current work

- [ ] Make the idle lock state visible and verify real password authentication
      after wake, returning to the same applications with working keyboard,
      touchpad, and touchscreen. The protocol fixture cannot establish PAM
      authentication; the user must enter their password locally.

## Remaining hardware acceptance

- [ ] Repeat suspend/resume cycles in one running session on AC and on battery;
      preserve application identity, geometry, input, audio, and network recovery.
- [ ] Suspend with another VT active, then return to Hearthspace; verify seat
      ownership, deferred presentation, and release/reacquisition of lid control.
- [ ] Wake with the lid still closed and confirm there is no immediate re-suspend
      until a new open-to-closed transition.
- [ ] Test a dock and connected external monitor: lid closure keeps the desktop
      awake, and connecting/removing displays around sleep restores usable input
      and correctly protected outputs.
- [ ] Measure battery drain over a realistic longer suspend on this `s2idle`
      machine, recording elapsed time and battery energy before and after.

Retain aborted attempts separately from failed sleep/recovery cycles. Record
hardware, power source, connected outputs, actual kernel sleep counters,
presentation/input assertions, and owned-session cleanup for every run.

## Later features

- [ ] Automatic idle lock, display dimming/off, and optional idle suspend with
      application idle inhibitors and configurable AC/battery behavior.
- [ ] Battery status and low/critical-battery notifications and policy.
- [ ] Graphical suspend action and power settings, including an explicit policy
      for the power button and lid behavior.
- [ ] Power profiles and hibernation, after host support and recovery are tested.

The host currently owns automatic idle and power-button behavior. These items
are separate features; they are not implemented by the lid coordinator.
