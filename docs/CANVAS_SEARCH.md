# Canvas search

Find an offscreen window without remembering where you left it. Canvas search
temporarily gathers matching objects around a text field, keeping their live
contents and identities. Dismissing it returns to your arrangement.

Choose **Search canvas** from the first Canvas tools button, then click empty
canvas to open the field there. **Meta+K** opens it near the upper center of
the active output. Applications keep their own slash and Ctrl+K input.

Type to find desktop app shortcuts, files, folders, open windows, system controls,
and the available canvas tools together. Every query word must match, in any
order and without case sensitivity. File and folder results come from the XDG
Desktop directory; search indexes their names and kinds, never their contents.
An empty query shows desktop items, controls and tools. Windows join the results
once the query contains text, ordered from front to back after desktop items.

Up and Down cycle through results. Enter or a click activates the selected item:

- App shortcuts launch an app; files and folders use their existing default opener.
- Window previews focus the existing window and center it on the search output;
  frame-attached windows open at their existing edge.
- System controls open their existing dropdown or Apps List.
- Canvas tools select their mode or open the canvas map.

Escape, the close button, or an empty-canvas click dismisses search. Search stays
the selected tool, so another background click opens a fresh query. Choosing
desktop selection, Pan canvas, or window-group selection leaves Search mode.
Apps List keeps its separate Meta-tap and Super+Space shortcuts.

## Presentation and input

The 420 by 54 logical-pixel glass field stays near the click. Results go below
it in the upper half of the output and above it in the lower half. Compact
outputs reduce margins. No matches appears inside the field's panel, on the
same side as results. Native text editing, keyboard focus, accessibility names,
and the empty-state live region remain available.

Pointer hover raises desktop tiles and control/tool cards independently of
keyboard selection.
Selected window previews have a two-pixel accent outline with five pixels of
spacing, so selection remains visible even for small previews.

Existing window buffers, decorations and desktop icons move into a temporary
spatial grid. Nonmatching objects move beyond the nearest output edge and fade.
Position and size transitions use 360 ms cubic easing; opacity transitions take 220 ms.
Query changes continue from the currently displayed position. Activating a
window animates from its preview directly into its final view, trimming distant
invisible travel. Preview clicks never reach the app's contents.

Search does not resize clients or write presentation positions into the saved
canvas layout. Dismissal restores the original arrangement and viewport.
Windows and desktop items retain their live content while displayed as results.
The normal corner controls are hidden during search and return on dismissal.
Canvas panning, zooming and object dragging are suspended while search is open.

Search observes `org.gnome.desktop.interface enable-animations` when GSettings is
available. Disabling it removes search motion; other hosts default to animations.
`HEARTHSPACE_REDUCED_MOTION=1` also disables these transitions, including in
isolated tests. The preference is read off the UI thread and requires no GNOME
session or new service.

## Ownership and verification

`hearthspace-common::canvas_search` owns the result layout and easing math.
The compositor's `canvas_search` module owns sessions, matching, selection,
temporary transforms, input isolation and window activation. The shell's
`ui/canvas_search` module owns the native field and live control previews.
No model or handwriting runtime is included.

Control preview surfaces stay allocated across searches to avoid repeated
native-window creation on the interaction path. The compositor hides them
outside the active search; inactive and unmatched card contents are stashed
from accessibility. The private text field remains specific to each session.

Private command-socket automation can use `canvas-search-open`,
`canvas-search-status`, and session-guarded updates:

```text
canvas-search-open {"x":640,"y":260,"output":"HEADLESS-1"}
canvas-search-update {"session":1,"query":"terminal"}
canvas-search-step {"session":1,"step":1}
canvas-search-activate {"session":1,"id":"window:3"}
canvas-search-close 1
```

Read the session and output names from status rather than assuming these sample
IDs. The status includes result identities, the field bounds, and canonical
versus displayed object geometry. Stale session commands and late surfaces are
rejected. Locking or losing the VT clears search and invalidates deferred actions.
Control activation uses a short-lived, one-use compositor token before the shell
opens a dropdown; desktop file/app activation retains its existing token path.

Geometry tests cover dense result grids, compact and offset outputs, and motion
clipping. Native widget tests cover editing, selection, empty states and control
actions. The compositor's headless search regressions exercise real Wayland
surfaces, early typing, click isolation, transient positions and session cleanup.

After building the native binaries, the owned VT regression also exercises real
touchscreen contacts, multi-contact cancellation, device removal, and the native
text field:

```sh
scripts/canvas-search-touch-smoke --session "$PWD/artifacts/search-touch-1"
```

Use a fresh session directory and follow the ownership and recovery rules in
[native testing](./NATIVE_TESTING.md).
