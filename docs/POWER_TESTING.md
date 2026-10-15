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
scripts/power-smoke --session /tmp/hearthspace-power-1 --allow-system-suspend --wake-after 45 --timeout 75 --total-timeout 240
# A separate run proves the normal locker starts automatically from an unlocked desktop:
scripts/power-smoke --session /tmp/hearthspace-power-swaylock-1 --allow-system-suspend --locker swaylock --wake-after 45 --timeout 75 --total-timeout 240
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
`--locker swaylock` instead verifies the desktop begins unlocked with no existing
owned swaylock process, then lets the real `PrepareForSleep` path launch the
normal locker. After sleep it requires a swaylock child of the same compositor,
with the same UID and cgroup, preserved core processes, and confirmed lock/input/
output recovery. Capture remains denied. Without `--manual-unlock`, this mode
never waits for authentication, releases the lock, or kills its locker; final
cleanup stops the owned native session. Report that run as automatic-lock
acceptance with PAM unlock still untested.

To include local password authentication, explicitly opt in while someone is
present at the test machine:

```sh
scripts/power-smoke --session /tmp/hearthspace-power-manual-1 --allow-system-suspend --locker swaylock --manual-unlock --unlock-timeout 300 --total-timeout 600
```

After wake, wait for `manual-unlock-wait` in the test output, then type your
password on the machine's physical keyboard and press Enter. Do not enter it in
chat or pass it to a command. Then leave the input devices idle while the harness
checks the restored desktop. Authenticating before the recorded wait cannot
establish this test's locked-to-unlocked transition and will fail the attempt.
The harness first confirms the real owned swaylock and its presented shield,
then observes status and process identity only. It never types a password,
signals the locker, sends an unlock command, or substitutes the test fixture.
The same compositor must report an active, recovered, unlocked desktop and the
original locker must exit before normal capture and input checks resume. The
existing application identity, geometry, fresh-frame marker and exact typed-token
checks then verify the restored desktop. A locker disappearing while the shield
stays locked is a failure, not authentication evidence.

`--manual-unlock` requires `--locker swaylock`. Its wait defaults to 300 seconds
and accepts 1–900 seconds; `--unlock-timeout` without manual mode is rejected.
The default total budget grows from 240 seconds to 240 plus the unlock wait.
An explicit `--total-timeout` must reserve at least that sum. The human wait may
exceed the ordinary per-step timeout, but remains within the aggregate deadline
and reserves one normal step for post-unlock checks. Expiry follows normal owned
session cleanup and records no successful manual-unlock acceptance. This mode
is available for a later local run; its implementation alone is not PAM evidence.

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

## Acceptance record — 2026-10-14/15

Final implementation was exercised on bare-metal GPD hardware, AMD Strix Radeon
880M/890M (`1002:150e`, `amdgpu`), Ubuntu kernel `7.0.0-38-generic`, and one
2560×1600 eDP-1 display at scale 2. The kernel exposes `s2idle`; logind's maximum
inhibitor delay was 30 seconds. Both final runs used verified 45-second RTC alarms
and independently bounded native sessions on tty7, returning to the original tty2.

- `/tmp/hearthspace-power-swaylock-final`: passed automatic lock from an unlocked
  desktop, 45.186 seconds of kernel-recorded suspend, confirmed lock and native
  recovery readiness, owned real swaylock, preserved core process identities,
  capture refusal, and complete cleanup. No password or PAM unlock was used.
- `/tmp/hearthspace-power-fixture-final`: passed 44.479 seconds of recorded
  suspend, fresh locked presentation and recovery readiness, owner-only unlock,
  preserved terminal identity/geometry and core processes, visible green marker,
  and exact typed-token output after wake. Native eDP-1 frame IDs advanced from
  283 before sleep to 302 after wake and 344 after input. The retained PNGs were
  inspected alongside these semantic assertions.

Both manifests record `cleanup.complete=true`, stopped scopes, removed private
runtimes, and restored tty2. `t3code.service` and `t3code-tailnet.service` remained
active with their original PIDs. The test-owned RTC alarm was cleared. No global
logind, PAM, sleep-service, or persistent RTC configuration was changed.

Earlier attempts remain available: `/tmp/hearthspace-power-1` timed out before
sleep because a transient startup control timeout left the status permanently
marked degraded; healthy-state reconciliation and a regression fixed that bug.
Its cleanup completed. `/tmp/hearthspace-power-2` then passed a 45.060-second
fixture cycle, before the final recovery-readiness and sleep-latch changes. The
user reported waking that run early; its kernel counters prove sleep/recovery,
not the wake source. All three actual cycles incremented the kernel success
counter without increasing its failure counter.

Final full checks passed **438 Rust tests** and **196 Python tests**, including
real headless clients, private D-Bus logind fixtures, transport deadlines, lid
eligibility and stale-state rejection, inhibitor lifetimes, daemon replacement,
missing/duplicate sleep signals, input/presentation recovery, failed locker
startup, and authentication during pending sleep. Workspace formatting, Clippy
with warnings denied, all-target native/E2E/test-app builds, Ruff, and diff checks
also passed. Logs are `/tmp/hearthspace-power-final-{tests,python,clippy,build}.log`.

Physical lid closure, a connected external monitor/dock, and real PAM password
unlock were not exercised in those two runs. The subsequent physical-lid test
below covers lid closure; external-display policy and PAM unlock still require
their own observed runs.

### User-assisted physical lid closure — 2026-10-14/15

`/tmp/hearthspace-lid-physical-2` exercised the physical switch on the same GPD
hardware, with AC connected and no dock or external display. Hearthspace owned
tty7 in logind session 108. A bounded local recorder ran outside that desktop,
so evidence collection and cleanup survived the remote connection interruption.
It never issued a suspend command: the user closed and reopened the lid.

The recorder first proved the awake RTC interrupt, then armed a verified
180-second fallback alarm with a 90-second window for the user to close the lid.
These longer manual-test timings leave room for human coordination plus logind's
30-second inhibitor delay; they do not change `power-smoke`'s unattended limits.
The independent native watchdog was 660 seconds. The alarm was cleared after
recovery, before its deadline, so this run does not establish RTC-triggered wake.

Evidence establishes the complete lid-trigger sequence:

- ACPI and logind reported open → closed while tty7 was active, AC was online,
  and Hearthspace held its lid inhibitor.
- Hearthspace logged `lid closed in active undocked session; preparing lock`.
  The recorded D-Bus `Suspend(false)` sender, `:1.3390`, maps to this session's
  supervisor PID 561837, followed by `PrepareForSleep(true)`.
- Kernel suspend successes increased from 3 to 4, failures stayed at 0, and
  `CLOCK_BOOTTIME - CLOCK_MONOTONIC` increased by **22.877 seconds**. Reopening
  the lid and `PrepareForSleep(false)` were recorded on wake. AC briefly reported
  offline after wake before returning online; it was online at lid closure.
- The coordinator returned to `awake` with one sleep and one resume, its delay
  inhibitor rearmed, `lock_confirmed=true`, `recovery_ready=true`, and no final
  error. A transient control timeout cleared during recovery. Compositor status
  independently confirmed recovery readiness, redacted windows, and refused
  capture while locked. The core process identities survived, and the owned
  swaylock child remained running.

The user saw a dark screen with a cursor after reopening. This is consistent
with the configured `swaylock --color 111318` without an idle-visible indicator;
the observation alone does not prove authentication works. No manual unlock was
observed during the recorder's 150-second window. This run therefore establishes
physical lid-triggered sleep and protected native recovery, **not** PAM unlock
or post-unlock application interaction. A persistent idle indicator and password
feedback have since been implemented; local visual and PAM-unlock acceptance
remain pending the user's later test.

The recorder then stopped its owned session: `cleanup.complete=true`, tty2
restored, scope stopped, runtime removed, RTC alarm cleared, and both T3 services
preserved with their original PIDs. Retained evidence includes
`artifacts/physical-timeline.jsonl`, `artifacts/logind-events.log`,
`artifacts/results.json`, the pre-sleep PNG/metadata, manifest, binary hashes,
session log, and the temporary manual runner. No persistent host policy changed.

An earlier attempt, `/tmp/hearthspace-lid-physical-1`, expired its 45-second
close window without observing a lid closure. It recorded no sleep, cleared its
120-second alarm, and completed cleanup. It is retained as an aborted attempt,
not a failed suspend cycle.
