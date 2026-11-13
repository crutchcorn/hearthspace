# Wi-Fi and Bluetooth

Open **Wi-Fi settings** or **Bluetooth settings** in the frame, or find them in
canvas search. The dropdowns offer connection actions and a button to open the corresponding
Settings page. Pairing, discovery, forgetting devices, and details live in
Settings. Add or move the Bluetooth frame item through **Change layout** if your saved layout
predates it.

## Wi-Fi

The panel shows nearby networks known to NetworkManager, connection status, and
signal strength through the Wi-Fi icon’s arcs rather than a percentage label.
In Settings, **Refresh networks** requests a scan; merely opening a panel does
not scan or connect. The switch enables or disables Wi-Fi. A hardware block
must be cleared with the machine's switch or airplane-mode controls.

Choose a network row to use a saved connection or join an open, WPA/WPA2 personal,
or WPA3 personal network. A new secured connection opens a masked password
dialog. Canceling it leaves the connection unchanged. In Settings, the chevron opens details
with the connection actions; **Back** returns to the list. **Disconnect** targets the
selected active connection; **Forget** removes matching saved profiles. Forget
has no effect when the network has no saved profile.

New enterprise, WEP, and hidden-network configuration is not provided here.
Configure those with another NetworkManager client; compatible saved profiles
can then be activated from Hearthspace. NetworkManager retains credentials and
connection preferences; Hearthspace does not write a separate wireless config.

## Bluetooth

The tray lists paired or connected devices for connecting and disconnecting.
Choose **Bluetooth settings**, then **Scan for devices** to discover nearby devices for
five seconds. Hearthspace releases only its own discovery session. Choose
an unpaired device to pair, or a paired device to connect/disconnect. In Settings, open its
chevron details to use
**Forget** to remove its pairing. Put the device into pairing mode first.

Pairing uses BlueZ's standard authentication agent for PIN entry, displayed
codes, confirmation, and authorization. The agent belongs only to the pairing
initiated by Hearthspace; it does not take over as the system's default agent.
Canceling a request closes its prompt. Entered passwords and PINs are not placed
in process arguments or application logs.

## Services and ownership

The Linux system bus must expose **NetworkManager** (`org.freedesktop.NetworkManager`)
and **BlueZ** (`org.bluez`). Install **Zenity** for toolkit-owned masked entry
and pairing dialogs. The host's normal D-Bus and authorization policy applies;
Hearthspace does not grant privileges, install policy overrides, or change
service configuration. Missing services, missing adapters, rejected actions,
and timeouts appear in the panel. Switches reflect observed service state rather
than assuming an action succeeded.

`hearthspace-desktop::wireless` owns asynchronous D-Bus access, bounded reads,
serialized actions, and user authentication. Settings and the shell adapt that
shared state to `ui-components::radio_panel`, composed from the same switches
and buttons as the Shared Controls Storybook. Read-only state refreshes every
three seconds. Network and device rows carry the service owner's identity;
replacing a row tears down its old input target so a held click cannot activate
its replacement. Authentication rechecks the network's service identity before
creating a connection.

## Development checks

Run the backend fixtures without access to real radios:

```sh
cargo test --locked -p hearthspace-desktop wireless -- --test-threads=1
cargo test --locked -p hearthspace-shell quick_panel -- --test-threads=1
```

The service fixtures start private D-Bus daemons with NetworkManager and BlueZ
interfaces. They test read-only snapshots, explicit writes, passwords,
authentication cancellation, daemon replacement, and pairing-agent callbacks.
The native widget tests cover pending/error state and device replacement during
a held press. Storybook provides Wi-Fi and Bluetooth Shared Controls stories
without contacting system services, including a compact tray variant. The tray
keeps its toggle and Settings link outside the scrolling connection list.

These tests do not establish hardware association, driver support, pairing with
a physical peripheral, or distribution authorization policy. Test those locally
on a spare connection and peripheral. Never disable or disconnect the only
network carrying a remote development session. To isolate UI/headless tests,
set `DBUS_SYSTEM_BUS_ADDRESS` to a private test bus or an unavailable owned socket;
this prevents test applications from controlling the host's radios.
