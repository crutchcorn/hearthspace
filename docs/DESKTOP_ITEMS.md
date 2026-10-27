# Desktop items

Every installed application in Apps List also has a permanent desktop shortcut.
Favorites remain a separate choice: removing a favorite does not remove its
desktop icon. Files and folders come from the user's configured XDG desktop
directory. These are the user's real files and installed applications.

## Select and move

Choose **Select desktop items** in Canvas tools, or press **Meta+V**. A click selects an icon; double-click or press Enter to open it.
Shift, Ctrl, or Super adds or removes an item from the selection. Dragging a
selected icon moves the selected items together. Drag empty canvas to select
intersecting icons with a rectangle. Hold Alt while dragging to align the
grabbed icon with the dot grid, preserving the spacing between selected items.
Pressing or releasing Alt updates the alignment even when the pointer is still.
The same grid aligns [window movement and resizing](WINDOW_MANAGEMENT.md).
Escape cancels a move and restores its starting positions, or clears selection when no drag is active.

With **Pan canvas** selected, dragging a canvas icon moves the view instead of
the icon. Return to **Select desktop items** to move or open it normally.
Frame-attached icons retain their normal behavior.

Tab and Shift+Tab move keyboard focus between desktop icons. Arrow keys move to
an icon in that direction; Space selects the focused icon. Keyboard focus has a
visible accent ring, while clicking preserves native focus without adding that
ring. The accessible button name includes the full filename or app name even
when the visible label wraps or truncates.

## Open items and manage favorites

Drop an app shortcut onto Apps List to add a favorite. The desktop icon returns
to its original position. Favorites remain pinned in the app dock in their saved
order, including while closed. Apps List keeps its normal alphabetical catalog.
Files and folders use GIO to select the user's default application. Both Open
and Open With ask the compositor to launch the selected installed desktop entry,
using the same application environment and owned Snap scope as app shortcuts.
GIO expands the entry's file arguments; filenames are never executed as commands.

Drag selected files or folders onto an application's content to offer their
local file URIs through Wayland drag-and-drop or X11 XDND. Applications choose
whether to accept the files. Dropping onto an application leaves the desktop
icons in their original positions; dropping onto the canvas moves them. Escape
cancels the transfer without sending the files.

## File actions

Right-click an icon, or press Menu / Shift+F10 while it has keyboard focus, to
open its context menu. Files support Open, Open With, Cut, Copy, Rename, Move to
Trash, and Properties; folders also support Paste Into Folder and New Folder.
Application shortcuts offer Open, favorite management, and Properties.
Right-click empty canvas for New Folder, Paste, Select All, Arrange Icons,
Open Desktop Folder, Background, Layout, and Settings. Menus support arrow keys,
Enter, and Escape, remain inside the output, and scroll on smaller displays.
Open With opens a separate submenu beside its parent, flipping left near the
output edge. Left or Escape returns to the parent. Properties opens the file
manager’s properties window, including the desktop entry for an app shortcut.

Cut and Copy use the ordinary clipboard's file URI formats, including the GNOME
copied-files format for Cut. Paste preserves existing destination files by
choosing a copy name; Rename refuses to replace another entry. Trash uses GIO's
Linux trash implementation. File operations run off the UI thread, and failures
remain visible in the menu. Closing a menu during a transfer dismisses the menu;
it does not cancel a file operation already in progress.

## Placement and appearance

Canvas icons stay beneath application windows and move and zoom with the canvas.
Each is an ordinary Xilem-rendered shell surface with an 88 by 96
logical-pixel tile. Native file, folder and Settings glyphs use filled pale
accent variants over a photo; labels become white with shadows. Real application
artwork and file previews retain their colors. Themes and accent changes update
existing surfaces.

## Launch and artwork contracts

Launch preparation runs outside the compositor event loop, with at most four
workers and an eight-second preparation/launch deadline. Native D-Bus handlers
keep a supervised GIO connection until the application acknowledges the file
request. Snap handlers inherit their prepared scope instead of relying on an
unscoped bus activation. Directory and catalog changes are refreshed on a worker. A separate artwork worker reuses
GIO-validated GNOME thumbnails and reads embedded FLAC front covers when no
usable thumbnail exists. Files without usable artwork use their themed MIME
icons. Opening Settings and changing themes reuse the retained artwork.
The desktop supports up to 4,096 combined items; exceeding that limit reports
an error in Apps List while keeping its complete application catalog available.

Application artwork is shared by desktop tiles, Apps List, the dock, and matched
window decorations. Requests retain the desktop application's identity, so two
shortcuts declaring the same generic icon can still display different artwork.
Steam game shortcuts retain a usable custom icon. When a shortcut declares the
generic Steam icon, Hearthspace tries its installed `steam_icon_<appid>` and then
the game's local cached icon before falling back to Steam. Cache discovery is
bounded to known native, Snap, and Flatpak Steam locations; ambiguous artwork is
ignored. It does not download assets or rewrite desktop entries. PNG, SVG, and
JPEG decoding retains the application's existing size and allocation limits.

File previews do not write to the user's files or thumbnail cache. Lofty reads
FLAC picture metadata; the existing `image` decoder handles PNG and JPEG covers.
Reads stay within the first 16 MiB of metadata, each picture is limited to 8 MiB,
and raster dimensions/allocation are bounded. At most eight previews are loaded
per worker pass. Both successful and missing previews are retained until the
source or its validated thumbnail changes; file identity and nanosecond timestamps
also detect replacements with a preserved filename or modification time.

## Ownership and persistence

The compositor owns selection, hit testing, canvas positions, and drag movement.
The shell owns accessible controls, icons, labels, catalog discovery, and app
activation. This keeps dragging independent of shell repaint or IPC latency.
Compositor-owned surfaces negotiate server decoration ownership without drawing
a titlebar. This avoids initializing a hidden client frame for every icon.
Desktop surfaces never enter the app dock, window switcher or window-group selection.

Positions use stable application IDs and desktop filenames. They are recorded
alongside the existing spatial layout and saved on normal compositor shutdown.
Native sessions use the regular layout state file; nested or headless sessions
can opt in with `HEARTHSPACE_LAYOUT_PATH`. Missing applications do not erase
saved positions, so reinstalling an app restores its placement.

[Canvas search](CANVAS_SEARCH.md) can temporarily bring matching icons into view
without changing their saved positions. [Change layout](SHELL.md#change-layout)
can attach icons to an output edge as compact tiles. On the canvas,
[peeking](WINDOW_MANAGEMENT.md#covered-canvas-work) exposes covered icons and
returns them to their home positions when space becomes available.

Implementation: `hearthspace-shell/src/desktop_items.rs` discovers real objects;
`ui/desktop` coordinates shell surfaces and worker tasks; `ui/desktop_item`
paints the native tiles; the compositor's `desktop` module manages canvas
interaction. The headless desktop workflow exercises native input, accessibility,
application opening, and canvas transforms together.
