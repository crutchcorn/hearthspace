# Lid, sleep, and wake recovery

Start a native desktop through `hearthspace-session` (including
`cargo run -- --dev --no-watch-apps` from a TTY or the native agent harness). Its power coordinator uses
systemd-logind on the system bus. A standalone compositor, nested desktop, or
session started with `--no-desktop-services` does not manage host power.

Close the lid to suspend an active, undocked Hearthspace session. A dock or a
connected external display keeps the session awake. The coordinator claims
logind's `handle-lid-switch` inhibitor only while its own local VT session is
active, and releases it when another desktop takes over. It never adopts an
unrelated active session. Startup, VT activation, bus reconnection, and waking
with an already closed lid do not trigger another suspend: a new open-to-closed
transition is required. Power-button and automatic idle policy remain with the
host configuration.

## Battery status and warnings

When a system battery is present, the shell bar shows its percentage and a
battery icon; a lightning bolt indicates charging. Open the power menu for the
full charging state. UPower supplies the combined reading for systems with
multiple batteries. A UPS is identified as a UPS. Desktops and VMs without a
system battery or UPS use no battery-indicator space and receive no battery warnings;
peripheral batteries are not treated as the system supply. A missing UPower
service or invalid reading also hides the indicator, and monitoring reconnects
when the service becomes available.

Low and critical warnings appear through Hearthspace's private notification
service. UPower's configured warning levels are authoritative. Only when that
level is unknown does Hearthspace use 20% for low and 5% for critical. Charging
and external-power readings suppress warnings. Warnings are deduplicated until
external power, charging, or meaningful charge recovery rearms them; critical
warnings can escalate independently. Daemon reconnects retain this state.
These warnings do not change the host's critical-battery action or initiate
suspend, hibernation or shutdown.

The [UPower display device](https://upower.freedesktop.org/docs/UPower.html)
and [device properties](https://upower.freedesktop.org/docs/Device.html) define
the aggregate, charging states and warning levels.

## Suspend from the shell

Choose **Power → Suspend** in a native managed session. This action is also
available on desktops without batteries when logind permits suspend. The menu
disables it with an explanation when sleep is unsupported, authorization is
required, another session is active, or the desktop is not ready. Nested and
headless sessions cannot suspend the host through this menu.

The shell sends a bounded request through the supervisor's private
`power-control.sock`. The supervisor rechecks the active session and sleep
capability, presents the lock shield, and then requests noninteractive logind
suspend. Repeated clicks do not queue multiple sleeps; an ambiguous timeout is
not automatically retried. Request errors remain in the power menu. The socket
uses the same private session directory as the compositor control socket.

You can also run `systemctl suspend` in the desktop terminal. Normal
logind suspend requests from other programs use the same lock preparation path.
The coordinator holds a sleep **delay** inhibitor, immediately shields the
compositor on `PrepareForSleep(true)`, and starts the distribution's `swaylock`.
It releases the inhibitor after every connected output has presented the opaque
lock frame. Lid-initiated requests wait for this presentation before requesting
sleep. A missing locker executable leaves the shield in place. Only the first
lock client can adopt that shield; losing an established owner never allows
another client to take over or reveal the desktop.

A sleep preparation latch keeps the shield active even if the owner authenticates
during another service's sleep delay. If that owner finishes before sleep, wake
starts a fresh locker; pre-sleep authentication never reveals the desktop on wake.
After waking, the session stays locked. Authenticate normally with swaylock to
resume work. The coordinator rearms the sleep inhibitor and requests explicit
native recovery even if logind/libseat did not emit a VT deactivate/activate
pair. Recovery clears held input, cancels gestures and window grabs, refreshes
libinput and DRM/KMS, and retries transient failures for a bounded interval.
Previous lock frames cannot prove recovery: a new presentation generation must
reach every output, and the native input source must be available. No new retry
starts after a five-second recovery budget; an in-flight kernel/driver call
cannot be preempted by this userspace deadline. Applications and their window state stay in the same
session. If another VT is active, rebuilding waits for Hearthspace's activation.

The same ownership and lock rules apply after a system-bus/logind disconnect.
An established session is shielded, connection attempts are bounded and retried,
and reconnecting refreshes the display in case sleep signals were missed. No
bus `Unlock` signal or agent command bypasses the lock owner's authentication.

## Diagnostics and failure recovery

The private session runtime contains `power.json`. It records connection and
inhibitor state, active-session/lid ownership, phase, observed sleep/resume
counts, lock confirmation, native recovery readiness, and the last error. The session log records sleep
preparation, inhibitor release, wake recovery, and degraded conditions. Counts
represent observed transitions, not independent proof of physical sleep.

A delay inhibitor has a logind-defined maximum; it cannot veto an external sleep
request forever. If the compositor misses that budget, the coordinator records
the failure and keeps isolation in place. It does not claim a presented frame or
successful recovery. An inactive Hearthspace VT cannot present its shield until
reactivated, so it may consume the maximum delay while another desktop is active. Lid-initiated suspend is cancelled if lock presentation
fails, and a failed or timed-out suspend call is not retried until a new lid
closure. Root commands that bypass logind/inhibitors fall outside this contract.

If swaylock or display recovery fails permanently, the desktop remains shielded.
For an agent-owned test, stop that exact session with
`scripts/hearthspace-agent --session /absolute/run stop`; the harness restores
its original VT and ends only its owned applications. For normal sessions,
return to another VT to inspect the session log and restart the failed session.
There is deliberately no unauthenticated unlock command.

See [session locking](SESSION_LOCK.md), [native suspend acceptance](POWER_TESTING.md),
and the [logind D-Bus contract](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).
No global logind configuration, PAM policy, sleep service, or permanent RTC
setting is modified by Hearthspace.
