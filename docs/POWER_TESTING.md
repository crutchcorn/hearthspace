# Native suspend recovery acceptance

Use the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
to establish exclusive VT ownership and keep the agent/remote connection outside
the desktop under test. A physical suspend pauses the whole machine, including
network access. This test requires an explicit `--allow-system-suspend` flag and
an already enabled UTC RTC with no existing alarm. It never changes the host's
lid or sleep policy, overwrites a scheduled alarm, or disables sleep inhibitors.

Build the current native binaries and the lock owner fixture before taking a VT:

```sh
cargo build --locked -p hearthspace-session -p hearthspace-compositor -p hearthspace-shell -p hearthspace-settingsd --features hearthspace-compositor/udev
cargo build --locked -p hearthspace-compositor --example session-lock-fixture
scripts/power-smoke --session /tmp/hearthspace-power-1 --allow-system-suspend --wake-after 20 --timeout 60 --total-timeout 240
# A separate run proves the normal locker starts automatically from an unlocked desktop:
scripts/power-smoke --session /tmp/hearthspace-power-swaylock-1 --allow-system-suspend --locker swaylock --wake-after 20 --timeout 60 --total-timeout 240
```

Use a fresh artifact directory for every attempt. Acceptance currently requires
one connected display, Foot, `rtcwake`, noninteractive sudo, logind's unattended
suspend capability, and readable `/sys/power/suspend_stats` counters. Ordinary
regression tests never suspend the machine:

```sh
python3 -m unittest discover -s scripts/tests -p 'test_power*.py' -v
```

The smoke test first waits for a three-second RTC interrupt **while the machine
stays awake**. That proves the alarm path works; it cannot prove that firmware
will wake from sleep. It then starts an owned native session with the independent
systemd watchdog, launches a terminal, and records its window, core process
identities, visible marker and page-flip metadata. The protocol fixture owns a
test lock, including rejection of a second locker. This fixture can release only
its own lock; it cannot authenticate or unlock swaylock.

Immediately before calling `systemctl suspend`, the test arms an RTC wake alarm
and verifies the kernel's readback. The default alarm is twenty seconds away;
logind delay inhibitors consume part of that interval. The test requires exactly
one successful kernel suspend cycle, no increased failure counter, and measurable
suspended time from `CLOCK_BOOTTIME - CLOCK_MONOTONIC`. A command returning
success or an already expired alarm does not establish that sleep occurred.

After wake it requires the power coordinator to have processed both sleep and
resume, reacquired its inhibitor, and confirmed a newly presented lock shield.
Both coordinator and compositor must report `recovery_ready`, which requires
restored physical input, newly presented outputs and no unresolved backend error.
Locked status must redact windows and capture must remain denied. Only then does
the fixture release its own lock. The same compositor, supervisor, settings
service, shell, terminal identity and geometry must survive. Fresh native frames
must show the terminal's marker, and typing must produce an exact token in that
same terminal.

The default `--locker fixture` follows the owner-unlock workflow above.
`--locker swaylock` instead verifies the desktop begins unlocked with no existing
owned swaylock process, then lets the real `PrepareForSleep` path launch the
normal locker. After sleep it requires a swaylock child of the same compositor,
with the same UID and cgroup, preserved core processes, and confirmed lock/input/
output recovery. Capture remains denied. This mode never authenticates, releases
the lock, or kills its locker; final cleanup stops the owned native session.
Report this as automatic-lock acceptance with PAM unlock still untested.

Normal and failure exits stop the owned native session and verify original-VT,
scope, runtime, and background T3 service recovery. The RTC alarm is cleared only
while its value still matches the test's alarm; a changed alarm is preserved and
reported as an ownership failure. The independent native watchdog bounds a
stalled agent/compositor **while the system is awake**; it cannot wake a machine
whose firmware fails to honor the RTC alarm.

Retain `artifacts/results.json`, `lock-fixture.log`, PNG/JSON pairs, `session.log`,
`session.json` and `binaries.json`. Report actual GPU, kernel, output scale and
suspend mode, including all failed attempts. This acceptance proves the real
logind sleep/resume path with either an established test lock or the automatic
real locker, depending on the selected mode. Swaylock authentication,
physical lid-switch operation, external-monitor policy,
and firmware wake behavior on other machines need separate evidence.
