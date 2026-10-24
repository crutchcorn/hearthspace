# Native suspend recovery acceptance

Use the [native control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
to establish exclusive VT ownership and keep the agent/remote connection outside
the desktop under test. A physical suspend pauses the whole machine, including
network access. This test requires an explicit `--allow-system-suspend` flag and
an already enabled UTC RTC with no existing alarm. It never changes the host's
lid or sleep policy, overwrites a scheduled alarm, or disables sleep inhibitors.

Run from the repository root after [dependency setup](DEPENDENCIES.md). Build
the current native binaries and the lock owner fixture before taking a VT:

```sh
cargo build --locked --target-dir target --workspace --all-targets --features udev,e2e,test-apps
cargo build --locked --target-dir target -p hearthspace-compositor --example session-lock-fixture
power_runs="$(mktemp -d -t hearthspace-power.XXXXXX)"
scripts/power-smoke --session "$power_runs/fixture" --allow-system-suspend --wake-after 45 --timeout 75 --total-timeout 240
# A separate run proves the normal locker starts automatically from an unlocked desktop:
scripts/power-smoke --session "$power_runs/automatic-lock" --allow-system-suspend --locker hearthspace-lock --wake-after 45 --timeout 75 --total-timeout 240
```

Use a fresh artifact directory for every attempt. Acceptance currently requires
one connected display, the installed `/etc/pam.d/hearthspace-lock` policy for
real-locker mode (see [graphical login installation](LOGIN_SESSION.md)),
Foot, `rtcwake`, noninteractive sudo, logind's unattended suspend capability,
and readable `/sys/power/suspend_stats` counters. Ordinary
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
its own lock; it cannot authenticate or unlock `hearthspace-lock`.

Immediately before calling `systemctl suspend`, the test arms an RTC wake alarm
and verifies the kernel's readback. The default alarm is forty-five seconds away;
logind delay inhibitors consume part of that interval. Preflight reads logind's
`InhibitDelayMaxUSec` and requires the wake delay to exceed its rounded-up maximum
by at least ten seconds. The harness rejects shorter alarms before touching the
RTC or taking a VT. Hosts with a delay over thirty-five seconds cannot meet this
margin within the harness's forty-five-second maximum, so unattended testing
aborts without changing system policy. The test requires exactly
one successful kernel suspend cycle, no increased failure counter, and measurable
suspended time from `CLOCK_BOOTTIME - CLOCK_MONOTONIC`. A command returning
success or an already expired alarm does not establish that sleep occurred.

After wake it requires the power coordinator to have processed both sleep and
resume, reacquired its inhibitor, and confirmed a newly presented lock shield.
Both coordinator and compositor must report `recovery_ready`, which requires
an available libinput source, newly presented outputs and no unresolved backend error.
Locked status must redact windows and capture must remain denied. Only then does
the fixture release its own lock. The same compositor, supervisor, settings
service, shell, terminal identity and geometry must survive. Fresh native frames
must show the terminal's marker, and typing must produce an exact token in that
same terminal.

The default `--locker fixture` follows the owner-unlock workflow above.
`--locker hearthspace-lock` instead verifies the desktop begins unlocked with no
existing owned `hearthspace-lock` process, then lets the real `PrepareForSleep`
path launch the normal locker. After sleep it requires a `hearthspace-lock` child
of the same compositor, with the same UID and cgroup, the selected compositor
sibling executable, preserved core processes, and confirmed lock, input, and
output recovery. Capture remains denied. Without `--manual-unlock`, this mode
never waits for authentication, releases the lock, or kills its locker; final
cleanup stops the owned native session. Report that run as automatic-lock
acceptance with PAM unlock still untested. Linux truncates the locker process
name to `hearthspace-loc`; the harness verifies that name and the executable
device/inode, parent, UID, cgroup, PID, and start time. A matching process name
alone cannot establish ownership. `binaries.json` retains the locker hash.
The old `--locker swaylock` spelling remains a compatibility alias selecting
`hearthspace-lock`; it does not launch swaylock.

To include local password authentication, explicitly opt in while someone is
present at the test machine:

```sh
manual_run="$(mktemp -d -t hearthspace-power-manual.XXXXXX)"
scripts/power-smoke --session "$manual_run" --allow-system-suspend --locker hearthspace-lock --manual-unlock --unlock-timeout 300 --total-timeout 600
```

After wake, wait for `manual-unlock-wait` in the test output, then type your
password on the machine's physical keyboard and press Enter. Do not enter it in
chat or pass it to a command. Then leave the input devices idle while the harness
checks the restored desktop. Authenticating before the recorded wait cannot
establish this test's locked-to-unlocked transition and will fail the attempt.
The harness first confirms the owned `hearthspace-lock` and its presented shield,
then observes status and process identity only. It never types a password,
signals the locker, sends an unlock command, or substitutes the test fixture.
The same compositor must report an active, recovered, unlocked desktop and the
original locker must exit before normal capture and input checks resume. The
existing application identity, geometry, fresh-frame marker and exact typed-token
checks then verify the restored desktop. A locker disappearing while the shield
stays locked is a failure, not authentication evidence.

`--manual-unlock` requires `--locker hearthspace-lock`. Its wait defaults to 300
seconds and accepts 1–900 seconds; `--unlock-timeout` without manual mode is rejected.
The default total budget grows from 240 seconds to 240 plus the unlock wait.
An explicit `--total-timeout` must reserve at least that sum. The human wait may
exceed the ordinary per-step timeout, but remains within the aggregate deadline
and reserves one normal step for post-unlock checks. Expiry follows normal owned
session cleanup and records no successful manual-unlock acceptance. A successful
run must observe local authentication; the implementation alone is not PAM evidence.

Normal and failure exits stop the owned native session and verify original-VT,
scope and runtime recovery. The current harness also compares the state and PIDs
of `t3code.service` and `t3code-tailnet.service`; other remote-agent services need
their own before/after checks. The RTC alarm is cleared only
while its value still matches the test's alarm; a changed alarm is preserved and
reported as an ownership failure. The independent native watchdog bounds a
stalled agent/compositor **while the system is awake**; it cannot wake a machine
whose firmware fails to honor the RTC alarm.

Retain `artifacts/results.json`, PNG/JSON pairs, `session.log`, `session.json` and
`binaries.json`, plus `lock-fixture.log` when using the fixture locker. Report actual GPU, kernel, output scale and
suspend mode, including all failed attempts. This acceptance proves the real
logind sleep/resume path with either an established test lock or the automatic
real locker, depending on the selected mode. Real-user PAM authentication needs
the manual-unlock mode. Physical lid-switch operation, external-monitor policy,
and firmware wake behavior on other machines need separate evidence.

## Additional acceptance

The automated suspend workflow does not close a physical lid, drain a battery,
select a display-manager session, or exercise every dock and monitor arrangement.
Keep those results separate, and record the environment in
[tested environments](TESTED_ENVIRONMENTS.md).

### Physical lid closure

Use an owned native session and a local participant. Keep a bounded recorder and
recovery controller outside the desktop under test. Confirm RTC ownership and a
fallback wake plan before asking the participant to close the lid; allow enough
time for coordination and the configured logind inhibitor delay. A custom manual
recorder does not change `power-smoke`'s 15–45 second wake-alarm limits.

Observe the lid's open-to-closed transition, active session, dock/external-display
state, Hearthspace's lock preparation, the suspend request and logind sleep/resume
signals. Require kernel sleep counters and elapsed suspended time, then the same
lock and recovery checks as the automated workflow. Opening the lid and seeing a
dark screen alone proves neither a recovered lock nor authentication. Observe
local unlock separately, and check preserved applications and physical input.

Record whether the user or RTC woke the machine. Clear only the still-owned alarm,
stop the exact test session and verify VT, scope, device and runtime cleanup.

### Graphical login and password unlock

Follow [graphical-login acceptance](LOGIN_SESSION.md#acceptance). A real GDM login
requires the user to select Hearthspace and authenticate locally. When observing
an existing login, verify the exact supervisor, compositor, locker and runtime;
attachment does not authorize replacing the locker or stopping that login.

Keep automated fixture unlock, automatic real locking and local password unlock
as separate assertions. Old swaylock results do not establish authentication for
the current `hearthspace-lock` client. If an observer times out before the user
unlocks, retain the timeout. A bounded continuation may observe the same verified
locker without another suspend; record it as a separate observation.

### Battery and power-menu controls

Compare the shell's battery text with the current UPower state. Exercise the
Power menu's Suspend action separately from a harness-issued suspend command:
record the activation, normal sleep/lock sequence and protected recovery. Use
local authentication for any claimed password-unlock result.

Headless power UI tests use private fake UPower services and a mock coordinator.
They cover charging, low/critical notifications, deduplication, charging
suppression, battery removal and unavailable-action explanations without
suspending the machine. Run them with the software-renderer environment in
[E2E testing](E2E_TESTING.md). These tests do not establish physical battery
behavior, firmware wake reliability or external-display policy.
