# Native browser, editor, and file-manager acceptance

These checks extend [daily smoke](DAILY_SMOKE.md) with real desktop applications.
Follow the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and keep other GPU tests stopped. Prepare fixtures before starting the session:
the compositor discovers desktop entries at startup. Launch applications through
the owned compositor or a terminal inside its native session so their processes,
display, and private D-Bus belong to the test.

## Temporary Firefox setup

Firefox Snap needs an explicit new profile under
`~/snap/firefox/common/hearthspace-native-probe-UNIQUE`. A new XDG configuration
directory alone does not isolate Firefox's legacy profile storage. Never inspect,
reuse, or modify an existing browser profile for these tests.

Copy `scripts/fixtures/daily-browser/` into a `fixtures` subdirectory of that new
profile, and retain another copy in the run artifacts. These local pages make no
network requests. The optional profile-local `user.js` setting
`user_pref("browser.aboutwelcome.enabled", false);` suppresses the initial welcome
page ([Mozilla onboarding documentation](https://firefox-source-docs.mozilla.org/browser/components/asrouter/docs/first-run.html)).

Create `~/.local/share/applications/dev.hearthspace.FirefoxProbe.desktop` only if
it does not already exist. Substitute the absolute new profile and fixture URL:

```ini
[Desktop Entry]
Type=Application
Name=Hearthspace Firefox Probe
Exec=/snap/bin/firefox --no-remote --profile "/absolute/new/profile" "file:///absolute/new/profile/fixtures/index.html"
TryExec=/snap/bin/firefox
Terminal=false
Icon=/snap/firefox/current/default256.png
Categories=Network;WebBrowser;
X-SnapInstanceName=firefox
```

`X-SnapInstanceName=firefox` is the key consumed by Hearthspace's catalog. It makes
the compositor create the Snap-accessible Wayland listener before spawning the
app. The `--profile` path and `--no-remote` argument keep this probe separate from
ordinary browsing ([Mozilla command-line reference](https://firefox-source-docs.mozilla.org/browser/CommandLineParameters.html)).
Validate the entry with `desktop-file-validate` before native startup.

The first native run uncovered an additional Snap integration requirement:
Snap tries to create its tracking scope through the session bus, while the
private Hearthspace bus intentionally has no user systemd manager. Firefox
therefore exited before displaying a window. The Snap-accessible Wayland socket
alone does not establish browser support. Snap's
[tracking implementation](https://github.com/canonical/snapd/blob/2.76/sandbox/cgroup/tracking.go)
and [launcher](https://github.com/canonical/snapd/blob/2.76/cmd/snap/cmd_run.go)
recognize a preexisting `snap.firefox.firefox-UUID.scope`. A diagnostic may create
that scope using the host user manager, restore the private runtime and bus
before executing Firefox, and impose a short `RuntimeMaxSec`. Record and stop
that exact scope separately: a user-manager scope leaves the harness's PAM
session cgroup. Never substitute the host desktop bus for Firefox's private bus
or disable Snap confinement.

The prepared run has an ownership-checked `launch-firefox-scope.py` diagnostic
with `start` and `stop` commands. Run `start` only from its owned native terminal;
run `stop` before stopping the native harness. Its scope expires after 300
seconds and is recorded in `firefox-scope.json`. This is a diagnostic, not yet an
integrated application-launch or cleanup guarantee. Snap also expects the
document portal at `/run/user/UID/doc`, whereas Hearthspace mounts its private
portal beneath the private runtime. Treat file-chooser support as unverified
until the exact file-selection workflow below passes with confinement intact.

Record the exact entry path and SHA-256, new profile path, a unique ownership
marker inside that profile, and whether the compositor's dedicated
`~/.local/state/hearthspace/apps/dev.hearthspace.FirefoxProbe.desktop` directory
already existed. Keep this record in the run's `app-probe-ownership.json`.
The prepared `/tmp/hearthspace-daily-apps-native-1` run includes this manifest,
copies of the fixtures, a `workspace` directory, and an ownership-checking
`cleanup-app-probes.py`. Its manifest records the actual unique profile path.

## Workflows and evidence

Launch `dev.hearthspace.FirefoxProbe.desktop` through the agent's `launch` command
or the shell entry named **Hearthspace Firefox Probe**. Preserve screenshots and
their native presentation metadata for the following checks:

- Type or clipboard-paste a unique sentinel into **Browser text**, then click
  **Confirm text**. Assert the visible/accessibility output is exactly
  `Confirmed: ` followed by the sentinel. Use **Browser notes** for multiline text.
- Click **Open local link** and assert the new page contains
  `HEARTHSPACE_LOCAL_LINK_OK`. Navigate back and verify the original page returns.
- Use **Choose a local proof file** to select the owned profile's
  `fixtures/upload-proof.txt`. Assert the page reports the filename, byte count,
  and exact text including `HEARTHSPACE_UPLOAD_OK`. A dialog merely opening is
  not proof that file selection worked.

For Visual Studio Code, run this from the owned native Foot terminal, replacing
`RUN` with the absolute artifact directory:

```sh
code --new-window --user-data-dir RUN/code-user-data --extensions-dir RUN/code-extensions RUN/workspace
```

The separate user-data directory avoids forwarding the command to an ordinary
Code instance. Open `daily-note.txt`, enter a unique line, save it, and compare
the file bytes from the agent. Close and reopen it to verify persistence. Keep
the workspace and Code data in the run artifacts.

GNOME Text Editor and Nautilus are available as `org.gnome.TextEditor.desktop`
and `org.gnome.Nautilus.desktop`. From the owned terminal they can also open the
fixture directly:

```sh
gnome-text-editor --new-window RUN/workspace/daily-note.txt
nautilus --new-window RUN/workspace
```

In Text Editor, append a second unique line and verify the saved file's exact
bytes. In Nautilus, use its location entry to reach the workspace, create a new
folder, rename it, and verify the resulting directory from the agent. Exercise
ordinary file opening into the editor and clipboard transfer between browser,
editor, and terminal. All created or changed files must remain inside the owned
workspace. Record which actions were asserted, rather than treating mapped
windows or accepted shortcuts as success.

## Cleanup

Close the probe applications, stop the native session through its harness, and
verify `cleanup.complete`, original-VT restoration, and runtime removal. Check
that no Firefox process still has the exact new profile path in its arguments;
do not use a broad process-name kill that could affect ordinary browsing.

For the prepared run, execute:

```sh
python3 /tmp/hearthspace-daily-apps-native-1/cleanup-app-probes.py
```

The helper requires successful native cleanup, validates the exact profile
parent/name and ownership marker, refuses to remove a changed desktop entry,
and refuses while the owned Firefox profile is still in use. It removes only
that entry, the new Firefox profile, and the dedicated compositor app-profile
directory if that directory did not preexist. It retains run artifacts,
screenshots, fixture copies, and edited workspace files. For later runs, apply
the same ownership checks to their recorded paths; never remove a preexisting
entry or profile.
