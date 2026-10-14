# App Notes

This is a running compatibility log for apps tested through Hearthspace's app launcher and Wayland compositor.

## Tested Apps

| App | Source / Type | Notes |
| --- | --- | --- |
| Foot | Native Wayland terminal | Uses server-side decorations. |
| A11yTest app | Hearthspace test app, GTK4 | Our in-repo accessibility test app. Uses client-side decorations. |
| Micro | Terminal app | Launches through terminal app handling. |
| SmartGit | Desktop app | Earlier testing opened it in GNOME instead of Hearthspace. Needs re-test after launcher environment fixes. |
| Visual Studio Code | DEB install | Previously had issues opening through Hearthspace. Current launcher changes appear to resolve this by keeping normal user `HOME`/D-Bus behavior while retaining per-app XDG dirs. |
| Firefox | Snap install | Previously had issues opening because Snap AppArmor rejected the custom Wayland socket name. Fixed by using numeric `WAYLAND_DISPLAY=wayland-99` and preparing the Snap runtime Wayland socket link. |

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
- Snap apps may need special Wayland socket handling because Snap confinement restricts allowed socket paths.
- Terminal apps should go through `xdg-terminal-exec` when available, with `xdg-terminals.list` fallback behavior.
