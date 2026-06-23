# Application Search And Terminal Launching Plan

## Goals

- Replace hardcoded app launching with a standardized installed-application catalog.
- Add a GPUI-based shell search box that supports partial app search.
- Launch apps through the existing compositor command socket so window placement stays compositor-owned.
- Use a terminal selection mechanism modeled after `xdg-terminal-exec`, not `mimeapps.list`.

## Application Discovery

Applications should be discovered from Freedesktop Desktop Entry files in the XDG data hierarchy:

```text
$XDG_DATA_HOME/applications
$XDG_DATA_DIRS/*/applications
~/.local/share/applications
/usr/local/share/applications
/usr/share/applications
```

The catalog should recurse through those `applications` directories and derive Desktop Entry IDs from paths relative to each base directory, following Freedesktop precedence: higher-priority directories win over lower-priority duplicates.

Only `[Desktop Entry]` records should be considered for normal launching. Desktop actions can be added later.

## Application Visibility Rules

An app should be included only when all of these are true:

```text
Type=Application
Exec is present, unless DBusActivatable launching is implemented later
Hidden != true
NoDisplay != true
TryExec is either absent or resolves on PATH / exists as an absolute path
OnlyShowIn is absent or includes Hearthspace
NotShowIn does not include Hearthspace
```

This means `NoDisplay` apps and apps intended only for other desktops remain hidden.

## Search Behavior

Search should be case-insensitive and token-based. Every query token must match at least one searchable field.

Searchable fields:

```text
Name
GenericName
Comment
Keywords
Desktop Entry ID
Exec command name
Categories
```

Suggested score order:

```text
Name exact match
Name prefix match
Name word-prefix match
Keyword match
Name substring match
GenericName / Comment substring match
Desktop ID / Exec / Category substring match
```

Results should sort by score descending, then display name ascending.

## Shell Command Protocol

Keep existing compositor actions for pan, zoom, and accessibility logging.

Add a dynamic app launch command:

```text
launch-app <desktop-entry-id>
```

The GPUI shell client sends only the Desktop Entry ID. The compositor owns app resolution and launching so it can preserve spawn positioning, Wayland environment setup, and future policy decisions.

## App Launching

The compositor should resolve the Desktop Entry ID through the app catalog and parse its `Exec` field without invoking a shell.

Desktop Entry field codes should be handled safely:

```text
%f %F %u %U -> removed for launcher invocations with no file/URL target
%i -> omitted initially
%c -> app name
%k -> desktop file path
%% -> literal %
unknown field codes -> invalidate the launch
```

Every launched app should receive:

```text
WAYLAND_DISPLAY=wayland-99
```

GTK apps launched by Hearthspace should continue to receive Hearthspace's private GTK/GSettings decoration config where applicable.

## Terminal Apps

Do not use `mimeapps.list` for selecting a default terminal. `mimeapps.list` is for MIME types and URL scheme handlers, not terminal emulator preference.

Use the proposed `xdg-terminal-exec` model instead.

Preferred behavior:

1. If `xdg-terminal-exec` is available, launch `Terminal=true` apps through it.
2. If not available, read terminal preference lists using XDG config/data precedence:

```text
~/.config/hearthspace-xdg-terminals.list
~/.config/xdg-terminals.list
/etc/xdg/hearthspace-xdg-terminals.list
/etc/xdg/xdg-terminals.list
/usr/local/share/xdg-terminal-exec/hearthspace-xdg-terminals.list
/usr/local/share/xdg-terminal-exec/xdg-terminals.list
/usr/share/xdg-terminal-exec/hearthspace-xdg-terminals.list
/usr/share/xdg-terminal-exec/xdg-terminals.list
```

Terminal candidates must be visible application Desktop Entries with `Categories` containing `TerminalEmulator`.

When wrapping a command manually, prefer terminal metadata in this order:

```text
TerminalArgExec
X-TerminalArgExec
X-ExecArg
fallback: -e
```

## GPUI UI Shape

The current shell bar is forcibly configured by the compositor as a fixed-height `SHELL_BAR_APP_ID` surface. A full result list should eventually use a separate shell launcher surface with its own app ID and screen-space placement.

Initial implementation can put a compact search box directly in the bar and show a small inline result strip, but the long-term design is:

```text
Shell bar search affordance
-> GPUI launcher palette shell surface
-> search box + result list
-> Enter/click sends launch-app <desktop-entry-id>
```

## Testing

Automated tests should cover:

```text
Desktop Entry parsing
Visibility filters
XDG duplicate precedence
Partial search scoring
Exec field-code handling
Terminal preference parsing
ShellCommand launch-app parsing
```

Manual checks:

```text
Search "calc" launches Calculator
Search "code" launches Visual Studio Code if installed
Search "foot" launches Foot
NoDisplay apps do not appear
OnlyShowIn apps not listing Hearthspace do not appear
Terminal=true apps use xdg-terminal-exec or xdg-terminals.list fallback
```
