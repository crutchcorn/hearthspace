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

- Server-side decorations are negotiated by the app/toolkit and drawn by Hearthspace when applicable.
- Client-side decorations are drawn by the app/toolkit itself.
- Snap apps may need special Wayland socket handling because Snap confinement restricts allowed socket paths.
- Terminal apps should go through `xdg-terminal-exec` when available, with `xdg-terminals.list` fallback behavior.
