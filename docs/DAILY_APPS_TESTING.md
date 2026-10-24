# Native browser, editor, and file-manager acceptance

These checks extend [daily smoke](DAILY_SMOKE.md) with real desktop applications.
Follow the [native-control skill](../.agents/skills/hearthspace-native-control/SKILL.md)
and keep other GPU tests stopped. Prepare fixtures before starting the session:
the compositor discovers desktop entries at startup. Launch applications through
the owned compositor or a terminal inside its native session so their processes,
display, and private D-Bus belong to the test.

## Temporary Firefox setup

Use the native Firefox package described in [dependency setup](DEPENDENCIES.md)
for browser, file-chooser and URI acceptance.
Keep a separate Snap probe to verify confined catalog launch and owned-scope
cleanup. Both need an explicit new profile: a new XDG configuration directory
alone does not isolate Firefox's legacy profile storage. Never inspect, reuse,
or modify an existing browser profile for these tests.

Run setup commands from the repository root. Prepare a new run and local fixtures:

```sh
app_run="$(mktemp -d -t hearthspace-apps.XXXXXX)"
mkdir "$app_run/native-firefox-profile" "$app_run/workspace"
cp -R scripts/fixtures/daily-browser "$app_run/native-firefox-profile/fixtures"
cp -R scripts/fixtures/daily-browser "$app_run/browser-fixtures"
```

Keep the value of `app_run` for startup, application commands and cleanup. These
local pages make no network requests. The optional profile-local `user.js` setting
`user_pref("browser.aboutwelcome.enabled", false);` suppresses the initial welcome
page ([Mozilla onboarding documentation](https://firefox-source-docs.mozilla.org/browser/components/asrouter/docs/first-run.html)).

### Native package

Create `dev.hearthspace.NativeFirefoxProbe.desktop` in
`$HOME/.local/share/applications` only if absent. These interactive instructions
use the harness without `--isolated-profile`; a different XDG data layout needs
entries in the compositor's actual catalog search path. Substitute the absolute value of `app_run` in this desktop entry;
`/absolute/RUN` is a placeholder, not a shell variable:

```ini
[Desktop Entry]
Type=Application
Name=Hearthspace Native Firefox Probe
Exec=/usr/bin/firefox --no-remote --profile "/absolute/RUN/native-firefox-profile" "file:///absolute/RUN/native-firefox-profile/fixtures/index.html" %u
TryExec=/usr/bin/firefox
Terminal=false
Icon=firefox
Categories=Network;WebBrowser;
MimeType=text/html;x-scheme-handler/http;x-scheme-handler/https;
```

The `%u` argument lets the URI portal pass a requested URL. When testing URI
launch, close the first owned Firefox instance before selecting this entry:
`--no-remote` deliberately refuses to reuse its open profile. Verify both a
successful portal response and the requested URL in the resulting browser tab.
Do not change the host's default browser. Use `ask=true` on the OpenURI request
to select the temporary entry explicitly.

After creating the probe entries, refresh their MIME registration:

```sh
probe_apps="$HOME/.local/share/applications"
update-desktop-database "$probe_apps"
gio mime x-scheme-handler/https
```

The chooser uses this cache, so `MimeType` in the entry alone is insufficient.
Confirm the native probe appears as an HTTPS handler and the default browser is
unchanged. The Snap entry below is a launch probe without MIME registration. This command
regenerates `mimeinfo.cache`; it does not set defaults in `mimeapps.list`.

Harness runs also isolate the portal's `XDG_DATA_HOME`. Once native startup
reports its owned runtime, copy the two probe entries into
`RUNTIME/activated-app-profile/data/applications/` and run
`update-desktop-database` on that directory too. Verify the runtime against
`$app_run/session.json`, require destination entries to be absent, and compare the
copies with the hashes in `app-probe-ownership.json`. Record their exact private
paths in the manifest. Check `gio mime` with the portal's own `XDG_DATA_HOME` and
`XDG_DATA_DIRS`; registering only the user's ordinary applications directory
does not make a probe visible to an isolated portal. These private copies and
their cache are removed with the owned runtime at shutdown.

### Snap package

Create its new profile under
`~/snap/firefox/common/hearthspace-native-probe-UNIQUE` so Snap can access it. Copy
`scripts/fixtures/daily-browser/` into that profile's `fixtures` subdirectory too.
Create `dev.hearthspace.FirefoxProbe.desktop` in the same applications directory
only if it does not already exist. Substitute the absolute new profile and
fixture URL:

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
Validate both entries with `desktop-file-validate` before native startup.

A Snap-accessible Wayland socket is only part of the launch contract. Use
Hearthspace's [owned app-scope integration](SNAP_APPLICATIONS.md) through the
catalog, then verify its owner service, `BindsTo` relationship, scope descendants
and cleanup. The host user manager creates the tracking scope; the app itself
receives the Hearthspace runtime and selected session bus. Do not substitute the
host desktop bus for the private test bus or disable Snap confinement. A direct
terminal or portal launch is a separate path and cannot stand in for catalog
scope acceptance. Compositor-mediated file launches also use the scope wrapper.

Snap expects the document portal at `/run/user/UID/doc`. A private harness session
mounts its portal beneath its private runtime, so confined Firefox file selection
through that portal remains an acceptance gap. A mapped browser window is not
evidence for it. Installed logins use the shared user bus and require separate
portal acceptance; do not generalize a private-harness result to that environment.

Before launching, record each entry's exact path and SHA-256, each new profile
path and ownership marker, and whether its dedicated compositor app-profile
directory already existed. The latter is under
`${XDG_STATE_HOME:-$HOME/.local/state}/hearthspace/apps/APP_ID`, using the
compositor's environment. Keep this record as `app-probe-ownership.json` in the
run directory. It is a manual ownership manifest; no repository command generates
or cleans up these interactive probes automatically.

Start an owned native session with `--session "$app_run"` after preparing the
entries, following [native testing](NATIVE_TESTING.md). Choose a watchdog that
covers the planned checks. Do not assume XDG isolation alone protects an
application's non-XDG profile storage.

## Workflows and evidence

### Automated Firefox window and zoom regression

After building the native binaries, run this separately from other native/GPU
work. It starts a fresh session with `--no-build` and always attempts owned
session cleanup:

```sh
firefox_run="$(mktemp -d -t hearthspace-firefox.XXXXXX)"
scripts/firefox-smoke --session "$firefox_run" --total-timeout 420
```

This fixture uses native `/usr/bin/firefox`, one active output, and its English
**Restore Down**/**Maximize** controls. It types an explicit `--no-remote --profile`
launch into the owned Foot terminal, inheriting the session's GTK settings.
Before onboarding, the fixture requests a floating size of 1000×760, bounded
by the output's usable space, and waits for committed, fully visible geometry.
This keeps first-run controls reachable when Firefox replaces its command-line
size during startup. The output must provide at least 800×600 usable logical
pixels. This setup does not count as CSD resize acceptance.
Only the fresh profile's optional **Continue**/**Not now** onboarding controls
are dismissed. Browser profiles, desktop entries, and default associations
outside the artifact directory are untouched.

The CSD maximize button must enter focused view, retain and repaint Firefox's
header, and hide the shell before any deliberate reveal. Each live system
widget is checked separately. Super+M then enters bounded canvas maximization
for the existing zoom checks.

Assertions cover real CSD restore/maximize, pointer-edge resizing, shell zoom,
three-finger touchpad and touchscreen zoom, fourth-contact cancellation, restored
floating geometry after cancellation, and F11 fullscreen remaining fixed during
canvas zoom, followed by restoration of the saved canvas geometry. Shell zoom
uses a pointer click at the observed accessible button
bounds after putting the browser's saved floating position far offscreen. It
must reveal the restored browser while preserving shell focus and the saved
canvas position. Maximized/fullscreen baselines wait for the client's configured
output geometry. Every independent zoom case resets to scale 1 to avoid hitting a
zoom limit. A fixed green rectangle in `zoom.html` must visibly scale in captured
pixels; viewport state changes alone cannot pass. Logs retain control results,
libinput traces, PNGs with page-flip metadata, and failure accessibility snapshots.

A fourth touchscreen contact must roll back the three-finger canvas transform
without opening the switcher. It can leave a fresh four-finger recognizer pending
until lift, so the check requires exact rollback while held and an idle gesture
state with no switcher after release. The shell-zoom setup also keeps the whole
green marker visible before zooming; comparing a clipped baseline would produce
an incorrect scale ratio.

Successful runs remove their marker-owned browser profile only after verified
session cleanup and no matching live profile process. Failed runs retain that
profile for diagnosis. The normal 420-second aggregate budget reserves a separate
45 seconds for native cleanup; each touch-worker reply also respects its step's
deadline.

This covers the fixture's native Firefox window, zoom and input assertions.
Browser compatibility, file selection, URI handling and other application workflows
still need the checks below. Record the Firefox version and binary provenance
with each run; environment coverage belongs in
[tested environments](TESTED_ENVIRONMENTS.md).

### Interactive application checks

Launch `dev.hearthspace.NativeFirefoxProbe.desktop` through the agent's `launch`
command or the shell entry named **Hearthspace Native Firefox Probe**. Use the
separate **Hearthspace Firefox Probe** entry for Snap scope acceptance. Preserve
screenshots and their native presentation metadata for the following checks:

- Type or clipboard-paste a unique sentinel into **Browser text**, then click
  **Confirm text**. Assert the visible/accessibility output is exactly
  `Confirmed: ` followed by the sentinel. Use **Browser notes** for multiline text.
- Click **Open local link** and assert the new page contains
  `HEARTHSPACE_LOCAL_LINK_OK`. Navigate back and verify the original page returns.
- Use **Choose a local proof file** to select the owned profile's
  `fixtures/upload-proof.txt`. Assert the page reports the filename, byte count,
  and exact text including `HEARTHSPACE_UPLOAD_OK`. A dialog merely opening is
  not proof that file selection worked.

For Visual Studio Code, run this from the owned native Foot terminal. Set
`app_run` there to the same absolute artifact directory used during setup; a
variable in the calling agent's shell is not automatically available in Foot:

```sh
code --new-window --user-data-dir "$app_run/code-user-data" --extensions-dir "$app_run/code-extensions" "$app_run/workspace"
```

The separate user-data directory avoids forwarding the command to an ordinary
Code instance. Open `daily-note.txt`, enter a unique line, save it, and compare
the file bytes from the agent. Close and reopen it to verify persistence. Keep
the workspace and Code data in the run artifacts.

When installed, GNOME Text Editor and Nautilus use the desktop-entry IDs
`org.gnome.TextEditor.desktop` and `org.gnome.Nautilus.desktop`. From the owned terminal they can also open the
fixture directly:

```sh
gnome-text-editor --new-window "$app_run/workspace/daily-note.txt"
nautilus --new-window "$app_run/workspace"
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

Use `app-probe-ownership.json` to remove only setup created for this run:

1. Require successful native cleanup and no live process using either recorded
   Firefox profile. Check the exact command arguments, not just the process name.
2. Verify each profile's expected parent, name and ownership marker. Preserve a
   path that was replaced, became a symlink or no longer matches its record.
3. Compare desktop-entry hashes before removing the owned entries. Leave a
   preexisting or changed entry for manual review.
4. Remove dedicated compositor app-profile directories only if the manifest says
   they did not preexist and they still belong to this run. Preserve screenshots,
   fixture copies, logs and edited workspace files as acceptance evidence.
5. Refresh the same applications directory with
   `update-desktop-database "$probe_apps"`. Verify the probe IDs are absent from
   `mimeinfo.cache` and the browser defaults remain unchanged.

The copies beneath the verified private runtime are removed by harness shutdown.
If any ownership check fails, retain the affected setup and report it; do not
replace these checks with a broad process kill or recursive deletion by name.
