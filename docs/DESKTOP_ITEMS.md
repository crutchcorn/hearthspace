# Desktop items

Every installed application in Apps List also has a permanent desktop shortcut.
Favorites remain a separate choice: removing a favorite does not remove its
desktop icon. Files and folders come from the user's configured XDG desktop
directory. Hearthspace does not create the experiment's sample files.

Choose **Select desktop items** in Canvas tools, or press **V** while the desktop
has focus. A click selects an icon; double-click or press Enter to open it.
Shift, Ctrl, or Super adds or removes an item from the selection. Dragging a
selected icon moves the selected items together. Drag empty canvas to select
intersecting icons with a rectangle. Hold Alt while dragging to align the
grabbed icon with the dot grid, preserving the spacing between selected items.
Pressing or releasing Alt updates the alignment even when the pointer is still.
The same grid aligns [window movement and resizing](WINDOW_MANAGEMENT.md).
Escape cancels a move and restores its starting
positions, or clears selection when no drag is active.

Tab and Shift+Tab move keyboard focus between desktop icons. Arrow keys move to
an icon in that direction; Space selects the focused icon. Keyboard focus has a
visible accent ring, while clicking preserves native focus without adding that
ring. The accessible button name includes the full filename or app name even
when the visible label wraps or truncates.

Drop an app shortcut onto Apps List to add a favorite. The desktop icon returns
to its original position. Files and folders open with `xdg-open`, using the
user's default application. They are not executed as commands. Directory and
catalog changes are refreshed on a worker; opening Settings, changing themes,
or refreshing the desktop does not read the contents of personal files.
The desktop supports up to 4,096 combined items; exceeding that limit reports
an error in Apps List while keeping its complete application catalog available.

Icons stay beneath application windows and move and zoom with the canvas.
Each is an ordinary Xilem-rendered shell surface with the website's 88 by 96
logical-pixel tile. Native file, folder and Settings glyphs use filled pale
accent variants over a photo; labels become white with shadows. Real application
artwork retains its colors. Themes and accent changes update existing surfaces.

The compositor owns selection, hit testing, canvas positions, and drag movement.
The shell owns accessible controls, icons, labels, catalog discovery, and app
activation. This keeps dragging independent of shell repaint or IPC latency.
Desktop surfaces have no window decorations and never enter the app dock,
window switcher, overview, or window-group selection.

Positions use stable application IDs and desktop filenames. They are recorded
alongside the existing spatial layout and saved on normal compositor shutdown.
Native sessions use the regular layout state file; nested or headless sessions
can opt in with `HEARTHSPACE_LAYOUT_PATH`. Missing applications do not erase
saved positions, so reinstalling an app restores its placement.

[Canvas search](CANVAS_SEARCH.md) can temporarily bring matching icons into view
without changing their saved positions. Frame attachment, layout editing and
peeking remain separate prototype features.

Implementation: `hearthspace-shell/src/desktop_items.rs` discovers real objects;
`ui/desktop` coordinates shell surfaces and worker tasks; `ui/desktop_item`
paints the native tiles; the compositor's `desktop` module manages canvas
interaction. The headless desktop workflow exercises native input, accessibility,
application opening, and canvas transforms together.
