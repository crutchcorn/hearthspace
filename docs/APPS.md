# App Notes

This is a running compatibility log for apps tested through Hearthspace's app launcher and Wayland compositor.

## Tested Apps

| App | Source / Type | Notes |
| --- | --- | --- |
| Foot | Native Wayland terminal | Keyboard launch/focus, Unicode input, browser clipboard transfers, and close/focus fallback exercised on native hardware. Uses server-side decorations. |
| A11yTest app | Hearthspace test app, GTK4 | Our in-repo accessibility test app. Uses client-side decorations. |
| Micro | Terminal app | Launches through terminal app handling. |
| SmartGit | Desktop app | Earlier testing opened it in GNOME instead of Hearthspace. Needs re-test after launcher environment fixes. |
| Visual Studio Code | DEB install | Native editing and saving exact Unicode bytes verified; semantic actions work alongside the browser accessibility tree. |
| Text Editor | GTK desktop app | Reopened and saved the editor's Unicode file in a native session. |
| Nautilus | GTK file manager | Native folder creation verified. GVfs mount cleanup belongs to session recovery; see the daily acceptance record for shutdown evidence. |
| Firefox | Mozilla native DEB | Native Unicode entry, browser-to-terminal copy, local navigation, and file selection with exact-content verification exercised. See [package setup](../CONTRIBUTING.md#native-firefox-package). |
| Firefox | Snap install | Catalog launch uses a managed Snap scope and a confinement-compatible Wayland socket. The private document-portal path remains a confined file-selection limitation; see [Snap applications](SNAP_APPLICATIONS.md). |

These checks cover specific workflows, not every application feature. Retained
native evidence and outstanding acceptance are recorded in
[daily usability](../todos/DAILY_USABILITY.md); repeatable browser/editor/file
checks are described in [native app acceptance](DAILY_APPS_TESTING.md).

## Notes

Normal sessions inherit the user's application configuration directories. An
editor launched from the app search and from a terminal therefore uses the same
preferences. Set `HEARTHSPACE_ISOLATE_APP_PROFILES=1` before starting a session to
use separate per-application XDG directories under Hearthspace's state directory.
The native test harness sets this explicitly so test launches can coexist with
applications on the original desktop. Hearthspace Settings always uses the
session daemon's configuration path.

Application single-instance behavior still belongs to the application. When
running two desktops with shared profiles, use the application's own new-instance
or profile option if it would otherwise reuse a window on the other desktop.

- Server-side decorations are negotiated by the app/toolkit and drawn by Hearthspace when applicable.
- Client-side decorations are drawn by the app/toolkit itself.
- Snap catalog launches require the session's owned application-scope service;
  direct terminal or portal launches do not use that wrapper. Socket compatibility
  alone does not establish confined portal/file access.
- Terminal apps should go through `xdg-terminal-exec` when available, with `xdg-terminals.list` fallback behavior.
