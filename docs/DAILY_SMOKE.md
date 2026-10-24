# Everyday native acceptance

`scripts/daily-smoke` exercises a fresh Hearthspace session on a spare VT and
retains evidence for each assertion. Build the native binaries first using the
[hardware testing guide](NATIVE_TESTING.md), and follow the
[native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md) to
coordinate ownership of the seat. This is a repeatable check of everyday desktop
behavior; a pass covers the listed assertions on the tested environment, not
long-term reliability or every application. The script always uses `--no-build`
and never attaches to an existing session. Keep GPU-heavy suites stopped during the run.

```bash
daily_run="$(mktemp -d -t hearthspace-daily.XXXXXX)"
scripts/daily-smoke \
  --session "$daily_run" \
  --timeout 45 --total-timeout 240 --captures 5 --notifications
```

Foot and Bash are required. `--notifications` additionally requires `notify-send`
and checks the session's notification service. Omit that flag to run the rest of
the workflow independently. Do not reuse a directory containing `session.json`
or an earlier `artifacts/results.json`.

The workflow checks:

- Super+Return launches and focuses Foot; typed commands work without a click.
- Explicit clipboard paste delivers exact Unicode bytes into a saved file.
- A Meta tap opens search, finds a uniquely titled running terminal, and selects
  its original ID without starting another instance.
- A window moved outside every output can be recovered by its stable ID.
- Maximize, fullscreen, return to maximized, and restore report the expected state.
- Sequential captures on every output are page-flip presentations with increasing
  frame IDs and the requested output identity.
- An unfinished settings client does not hold up a healthy settings query or
  compositor control and presentation.
- The private accessibility bus exposes named icon buttons; activating Canvas
  map and its Zoom out control changes the compositor viewport.
- With `--notifications`, a private notification is shown and dismissed through
  its accessibility action; the service reports user-dismissed history.

`--timeout` bounds each step, including startup. `--total-timeout` bounds startup
and interaction together, so starting another step cannot reset the overall
budget. Cleanup reserves a separate 45 seconds, and an independent systemd
watchdog covers agent death. A failed assertion still runs stop and verifies
original-VT restoration, runtime removal, and the harness cleanup result.
If submitting a running-window selection fails, diagnostics get up to eight
seconds from the remaining aggregate budget to save the resulting status,
capture, and accessibility tree before cleanup. Diagnostic failures never replace
the original assertion failure.

Results are written incrementally to `artifacts/results.json`. PNG captures have
adjacent metadata files; `unicode-received.txt` is the exact destination artifact,
`accessibility.xml` preserves semantic evidence, and optional `notify-send.log`
records the notification fixture. Startup logs, manifest, and binary provenance
remain in the session directory. Failed cleanup is retained alongside the
original failure rather than replacing it.

Run cross-application Wayland/X11 clipboard acceptance separately with
`scripts/clipboard-smoke`; this workflow only checks explicit Unicode paste into
Foot. Browser, editor, file-manager, suspend/resume, hotplug, and physical gesture
checks remain separate acceptance work. Browser, editor, and file-manager
workflows are described in [native app acceptance](DAILY_APPS_TESTING.md).

Use [physical suspend acceptance](POWER_TESTING.md) for sleep, locking, wake
recovery, application preservation, and RTC/watchdog ownership.

Use [compact shell acceptance](SHELL.md#acceptance) for immediate Meta typing,
icon placement, notification panels, and power-menu confirmation cancellation.
