# Widgets, canvas, and frame

This page defines the widget model being introduced. Until the implementation
lands, [Shell controls](SHELL.md) describes the current interface.

## The desk analogy

The canvas is a desk: files from the configured Desktop folder are the papers
on it, application windows are work in progress, and widgets are useful tools
placed beside that work. The frame is the desk's rim. Attaching a tool to the
rim keeps it within reach while the canvas pans and zooms; it does not turn
the tool into a different object.

Installed applications belong in the app catalog. Installing an application
does not put a shortcut on the desk. Apps remain available through the launcher,
search, and favorites. Actual files and shortcuts placed in the Desktop folder
remain ordinary desktop items.

Fresh layouts retain the starter frame arrangement. Those controls are widget
instances too: moving them to the canvas preserves their function, and removing
a canvas copy does not cause startup to recreate it. Adding another output may
initialize that output's starter arrangement; reconnecting a known output uses
its saved arrangement.

## Kinds and instances

A widget kind describes a capability: Notifications, Clock, Wi-Fi, Sound,
Brightness, Battery, Application tray, Power, Canvas tools, App dock, or App
launcher. The catalog describes what can be added, independently of what is
currently visible or whether a device or application has data to display.

A widget instance is one placed copy of that capability. Adding Clock twice
creates two independently movable clocks. Their readings may share a service,
but moving or removing one must not change the other. Adding a widget always
creates another instance, including when that kind is already on another part
of the canvas, on the frame, or on another output.

The app launcher is independently placeable. Its button opens the same installed
app catalog from either the canvas or the frame. Opening a launcher must not
require a dock or another launcher instance to be present.

## Placement and removal

Right-click empty canvas and choose **Add widget**, then choose a kind. The new
instance appears at that canvas location. The menu remains the recovery path
when every widget has been removed. Keyboard invocation of the canvas context
menu must offer the same actions.

**Change layout** moves an instance between canvas and frame, using the existing
edge guides and grouping interactions. Canvas widgets move and zoom with the
canvas; frame widgets stay attached to their output. Moving an instance retains
its identity, settings, and function.

A canvas widget can be removed without deleting a file, closing an application,
or disabling its service. Removal retires that instance. Adding the same kind
later creates a new instance rather than reviving a removed object's bindings.
Pending input or popup work for a removed instance must never activate its
replacement or another target.

## Ownership and persistence

Persist the instance's identity, kind, placement, and presentation preferences.
Keep that identity separate from a live Wayland surface, output connector name,
popup, and temporary animation pose. Recreating a shell surface or restarting
the session must preserve the user's layout without adding duplicate instances.
Disconnected outputs retain their saved arrangements.

The compositor owns placement, grouping, hit testing, and input isolation. The
shell owns accessible widget controls and their menus. Existing desktop services
remain the owners of app discovery, notifications, and device state. Multiple
widgets observe those services without creating competing service owners.

Existing saved layouts require an explicit migration: preserve placements
and groups while assigning instance identities, and do not recreate
removed widgets at startup or when an output reconnects. Installed-app canvas
shortcuts may stop being generated without removing applications, favorites,
Desktop files, or their saved positions.
