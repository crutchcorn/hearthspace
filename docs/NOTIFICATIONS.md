# Desktop notifications

Open **Notifications** to read active messages and recent history. Its bell
shows the active count, or **All clear** when nothing is active. Use a message's
action buttons to respond through its application, or **Dismiss** to close it.
Escape, the close button, or an outside click closes the panel. Closing the panel
does not dismiss its messages.

Messages arrive from applications on the session bus. They contain plain text
and optional actions; application-supplied images, sounds, and rich markup are
not supported. Recent history lasts only while the notification service runs.

## Service and application contract

Native sessions run `hearthspace-notifications` on the selected session D-Bus.
Installed sessions use the standard user bus and bind
`org.freedesktop.Notifications`. Native `--dev` sessions start a private bus so
notifications work alongside another logged-in desktop. The explicit
`--private-session-bus` option uses an existing dedicated bus instead.
The supervisor starts it before the portal backend and shell, records its PID,
restarts it at most twice, and stops it on logout. The daemon never takes over an
existing notification service. Nested sessions and `--no-desktop-services`
diagnostics do not start it.

Applications use the standard
[desktop notification D-Bus interface](https://specifications.freedesktop.org/notification/latest/protocol.html)
at `org.freedesktop.Notifications` and `/org/freedesktop/Notifications`. The
implementation uses [zbus](https://docs.rs/zbus/latest/zbus/) for D-Bus transport
and advertises only `actions` and `body`. Notification content is plain text;
markup, images, icons, sound files, URLs, and action keys are never executed by
the daemon or shell.

The shell displays the queue and recent history. Clicking an action emits
`ActionInvoked` to the original application's unique D-Bus connection, preserving
the exact action key. Nonresident actions then dismiss the notification. Only
the originating application can replace or withdraw its live notifications.
Closed notification IDs cannot be acted on again. `NotificationClosed` reports
reason 1 for expiry, 2 for user dismissal/action, and 3 for application withdrawal.

An explicit timeout is measured in milliseconds from receipt. Zero means no
expiry; the default is eight seconds, except critical notifications which remain
until dismissed. The `resident`, `transient`, and `urgency` hints are supported.
Transient notifications leave no history. History is held in memory for the
current daemon lifetime; it is not persisted across logout or service restart.

## Bounds and shell control

At most 16 notifications can be active, with 24 total active/history entries.
New notifications are rejected when the active queue is full; live items are
never silently discarded to make room. Text is bounded by UTF-8 byte length:
64 for application name, 128 for summary, 512 for body, and 48 for each action
label. Up to four actions are accepted; their nonempty keys must be at most 32
bytes and contain no control characters. Text truncation preserves UTF-8, and
control characters other than newlines are removed. The pending signal queue
and D-Bus inbound queue are bounded too.

The shell uses `$XDG_RUNTIME_DIR/hearthspace-notifications.sock`. This private
socket accepts one newline-terminated JSON request and returns one JSON line.
Clients may half-close their write side after sending the request. The daemon
allows 16 concurrent peers, each with a 250 ms lifetime, a 1 KiB request cap, and a
64 KiB response cap. Incomplete peers do not block other clients or expiry.

```json
{"command":"list"}
{"command":"dismiss","id":1}
{"command":"action","id":1,"key":"open"}
```

Successful mutations return `{"ok":true}`; failures return
`{"ok":false,"error":"..."}`. A list returns `ok: true` and `notifications` in
newest-first order. Each item has `id`, `app_name`, `summary`, `body`, `actions`
(`key`/`label` pairs), `urgency` (0–2), `active`, and `close_reason` (null while
active). Closed history entries have no actions. The shell polls away from its
UI thread and uses bounded IPC deadlines.

## Validation

```sh
cargo test -p hearthspace-notifications -p hearthspace-session
cargo clippy -p hearthspace-notifications -p hearthspace-session --all-targets -- -D warnings
```

The notification integration test launches its own `dbus-run-session`, daemon,
application connections, and control clients. It checks replacement, sender
ownership, action delivery, expiry, dismissal, application withdrawal, history,
and duplicate-daemon refusal without a display or host bus. The native harness
can additionally validate the shell's visible notifications and action buttons.

The session supervisor also has an opt-in test using the actual notification
executable alongside an existing provider on a separate disposable bus. Build
that executable explicitly, then pass its absolute path:

```sh
cargo build --locked -p hearthspace-notifications --target-dir target
HEARTHSPACE_TEST_NOTIFICATIONS_BIN="$PWD/target/debug/hearthspace-notifications" \
  cargo test --locked -p hearthspace-session --bin hearthspace-session \
  actual_notifications_bind_privately_and_leave_the_existing_provider_alive \
  -- --ignored --test-threads=1
```
