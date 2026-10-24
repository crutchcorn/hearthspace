# Session locking

Hearthspace implements `ext-session-lock-v1` with Smithay. The compositor owns
input isolation, output presentation, and the transition back to the desktop.
The workspace's `hearthspace-lock` client owns password authentication through
PAM. It uses GTK4 and `gtk4-session-lock` for the lock surfaces; the compositor
launches the binary beside its own executable, keeping installed releases together.

## Locking policy

Super+L, the shell Lock action, and the agent `lock` command launch
`hearthspace-lock`. The compositor arms an opaque shield before starting the
process, isolating input and automation even if launching the client fails. Locking
is confirmed only after an opaque lock frame has been presented on every output.
Agent status reports these separately as `locked` and `lock_confirmed`.

The lock screen shows a clock, the current user's name, and a password entry
over the configured background. Type your account password and press Enter or
activate **Unlock**. Failed authentication leaves the session locked and shows
feedback for another attempt. The selected account comes from the process's
Unix identity, not from an editable username or environment override.

Authentication uses only the fixed `/etc/pam.d/hearthspace-lock` service. Both
PAM authentication and account checks must succeed for the same Unix user.
Missing policy, expired credentials, account restrictions, and unsupported PAM
conversations fail closed; the locker never changes a password or opens a PAM
login session. See [installation](../CONTRIBUTING.md#graphical-login-installation)
for policy ownership and prerequisites.

An opaque screen without the password form can still be the compositor's protective
fallback while the locker starts or if it fails. It does not prove the password
UI is ready. Agents cannot capture the locked screen or supply the password;
visible feedback and real PAM unlock must be checked with local input.

Native outputs attach a lock generation to each queued frame. Only a matching
DRM page-flip completion satisfies the presentation barrier. A normal desktop
frame queued before the lock request cannot satisfy it. Outputs added during
locking join the barrier; outputs that disappear stop blocking it. With no
connected outputs, there is no presentation left to wait for. A newly connected
output clears `lock_confirmed` until its protected frame is presented. Pausing
the VT preserves the output set and cannot substitute for a page flip.

While locking or locked, the renderer draws only the owning lock client's surfaces and
the cursor over an opaque background. Missing or destroyed lock surfaces leave
that background visible. Keyboard and pointer input reach lock surfaces only.
Normal application shortcuts, popup grabs, and canvas gestures cannot redirect
input to the desktop.

If the locker exits or loses its connection, the session stays locked. Another
client cannot take ownership of that lock. Unlocking requires the owning
protocol object's authenticated request; there is no compositor unlock command.
For an owned native test session whose locker has failed, stop the session using
the native harness's documented recovery procedure. This ends its applications
and restores the previous VT.

## Agents and tests

Agent status remains available while locked and redacts normal window and view
state. Captures, app/window queries, synthetic input, clipboard changes, and
semantic accessibility operations are rejected. The user enters their password
through the real input devices. Stopping an owned test session remains available
through the harness.

The protocol test fixture supplies its own lock surfaces and releases its own
lock object without a password. This tests isolation, competing-client
rejection, restoration of application focus, and fail-closed disconnection. It
does not replace or weaken `hearthspace-lock` authentication. Headless tests use
completion of the offscreen render as their output presentation event; native validation
must exercise actual page-flip completion.

The lock client's ordinary tests cover its authentication transaction and session
state. `scripts/lock-auth-test` additionally exercises the unchanged PAM adapter
with a synthetic module inside a private mount namespace. Build the locker test
binary first and pass its absolute executable path:

```sh
cargo test --locked -p hearthspace-lock --no-run
sudo python3 scripts/lock-auth-test --uid "$(id -u)" --test-binary /absolute/path/to/test-executable
```

The runner replaces only its namespace's PAM directories, drops to the selected
non-root UID, and checks successful and rejected conversations, account checks,
identity changes, unsafe policy files, and missing-policy fallback. It neither
uses host credentials nor changes host PAM files. The ignored test refuses to
run outside this isolated fixture. This proves the PAM transport and policy
checks, not authentication against a particular host's account configuration.

For visual and password-widget checks, the `lock-preview` and
`secret-entry-regression` examples use ordinary GTK windows on the owned test
display. They never acquire or unlock a session lock. The latter exercises
masking after clearing, submission, retries, and empty/busy rejection.

For an owned native test session, build the same client as a standalone probe:

```sh
cargo build -p hearthspace-compositor --example session-lock-fixture
target/debug/examples/session-lock-fixture /path/to/owned/wayland-socket /path/to/owned/release unlock 60
```

The probe waits for the protocol `locked` event and verifies a competing locker
is rejected. After checking locked status and rejected agent actions, create the
fresh release file to let the probe unlock its own test object. The `disconnect`
mode instead drops the connection and must leave the desktop locked; stop that
owned harness session to recover. The `destroy-surfaces` mode removes only the
lock roles before waiting, exercising the opaque fallback. A timeout disconnects
without unlocking. This executable cannot unlock a lock owned by `hearthspace-lock`.

Locking is available on native and headless backends. The nested backend does
not advertise the protocol because it cannot prove the host compositor's
physical presentation or protect the host desktop.

## Dependencies and contributor rules

Build `hearthspace-lock` with the rest of the native binaries and install the
fixed PAM policy using `scripts/install-session install`. The supplied policy
includes the distribution's `login` authentication and account stacks. The
installer creates it as root:root, mode `0644`, and preserves an existing
administrator-managed policy. A different install prefix does not relocate PAM.
Development sessions build the sibling locker before starting the compositor;
locking never starts a compiler while the shield is active.
See [contributing](../CONTRIBUTING.md) for native library prerequisites and
[native testing](./NATIVE_TESTING.md) for session ownership and recovery.

- Keep authentication in the locker and display isolation in the compositor.
- Never acknowledge locking at render or queue submission time on native outputs.
- Keep the shield active when clients, lock surfaces, or rendering fail.
- Preserve the presentation barrier when changing output hotplug or VT recovery.
- Gate new automation and metadata APIs during locking as well as after confirmation.

The protocol requirements are defined in
[ext-session-lock-v1](https://wayland.app/protocols/ext-session-lock-v1).

Sleep preparation arms an opaque shield before starting `hearthspace-lock`.
A first locker may adopt that shield; an established owner can never be replaced after failure.
VT and sleep recovery invalidate prior presentation evidence and require fresh
locked page flips. See [lid and suspend handling](POWER.md) for coordinator policy
and failure recovery.
