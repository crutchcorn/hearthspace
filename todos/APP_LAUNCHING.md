# Application Search And Terminal Launching Plan

Remaining work. The catalog, visibility rules, search scoring, `launch-app`
protocol, Exec field-code handling, terminal selection, the shell-bar search
field, and the launcher palette dropdown are already implemented.

## Terminal Preference Parsing Test

Add automated coverage for terminal preference parsing. There is currently no
test exercising `terminal_preference_ids` / `terminal_command_for`, even though
the rest of the planned test categories are covered.

## Launcher Palette Shell Surface (done)

The bar's search field opens a dedicated launcher palette: a second shell
surface tagged with `LAUNCHER_APP_ID` that the compositor places just below the
bar in screen space. It shows the search results as a vertical dropdown, stays
closed while the field is empty, and clears the field (closing the palette) when
a result is selected:

```text
Shell bar search field
-> launcher palette shell surface (LAUNCHER_APP_ID)
-> vertical result list
-> Enter/click sends launch-app <desktop-entry-id>
```
