# Widgets, canvas, and frame

Widgets are independently placed shell controls. [Shell controls](SHELL.md)
describes their interface and the starter frame arrangement.

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

Right-click empty canvas and choose **Add Widget…** to open canvas search with
an internal widgets-only filter. The filter is not shown as a search token or a
separate menu. Type a widget name, then activate its add result. Ordinary canvas
search also includes those add results alongside its other results. Each
activation adds a new copy at the search's original canvas location.

The context menu remains available when every widget has been removed. The
Menu key or Shift+F10 opens it from the keyboard on empty canvas.

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

Existing saved layouts migrate placements, groups, and preferences to instance
identities. Widgets previously hidden stay removed. Automatic installed-app
icons disappear; explicitly placed app shortcuts remain. Applications,
favorites, Desktop files, and their saved positions are preserved.

## Search and instance identity

The editable search query contains only what the user types. The widgets-only
scope is session state, independent of query text. Clearing the query does not
clear that scope; a normal search starts with the full scope. Leaving the picker
restores the previously selected canvas tool.

Search entries such as `widget:clock` describe add actions in the catalog. They
are not placed instance identities. Layout status publishes a widget's opaque
`id`, its `kind`, and whether its native surface is `mapped`. Find a capability
by kind, then send the returned instance ID when editing that copy. Do not use
its kind as a layout target or assume a kind appears only once.

Instance IDs contain a saved namespace and monotonic sequence. Retiring a copy
keeps the sequence advanced, including when the collection becomes empty. The
widget collection owns identity and per-copy preferences; the existing frame
layout remains the sole placement and group owner for widgets, files, and
windows. Save those owners together. Moving a widget does not create another
placement record or a second service owner.

The starter battery and application tray retain their automatic absence policy.
Explicitly added copies remain visible when their service has no data, showing
an unavailable state or an empty tray placeholder so they can still be placed
or removed.
