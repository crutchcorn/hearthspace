# Application Search And Terminal Launching Plan

Remaining work. The catalog, visibility rules, search scoring, `launch-app`
protocol, Exec field-code handling, terminal selection, and the inline shell-bar
search UI are already implemented.

## Terminal Preference Parsing Test

Add automated coverage for terminal preference parsing. There is currently no
test exercising `terminal_preference_ids` / `terminal_command_for`, even though
the rest of the planned test categories are covered.

## Launcher Palette Shell Surface

Replace the inline result strip with a dedicated launcher shell surface that has
its own app ID and screen-space placement:

```text
Shell bar search affordance
-> launcher palette shell surface
-> search box + result list
-> Enter/click sends launch-app <desktop-entry-id>
```
