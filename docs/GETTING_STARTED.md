# Your first few minutes in Hearthspace

After [installing Hearthspace](LOGIN_SESSION.md), select it at the login screen.
If you're running it inside another desktop, some keys may be intercepted by
that desktop and system controls may be unavailable.

## Open something familiar

Tap **Super** (also called Meta or the Windows key), or click **Apps List** at
the bottom right. Type an app's name and press Enter. **Super+Return** opens
the `foot` terminal directly; install it with the
[desktop runtime packages](DEPENDENCIES.md#desktop-runtime).

Favorites stay in the dock when they're closed. A running app has an indicator;
click it to return to its window, and click again to cycle through its windows.
Apps List also searches running-window titles. The separate canvas search can
find desktop objects and controls as well.

## Spread out

Drag a window by its title bar to place it. You aren't limited to the part of
the canvas currently visible on screen.

| Navigation | How |
| --- | --- |
| Pan with a keyboard | **Super+arrow keys**. |
| Pan with a mouse | Hold **❖** and left-drag empty canvas. |
| Pan with touch | Slide three fingers on a touchpad or touchscreen. |
| Zoom with a mouse | Hold **❖** while scrolling. |
| Zoom with touch | Pinch with three fingers. |
| Change zoom from the map | Use its minus and plus buttons. |

**❖** means Meta, also called Super or the Windows key. Hold it before starting
the drag; releasing the mouse button ends the pan, and your selected tool stays
unchanged. For repeated panning without holding a key, choose **Pan canvas**
from Canvas tools or press **❖ + H**. Press **❖ + V** to return to desktop
selection. The [canvas map](CANVAS_MAP.md) provides another way to pan by dragging its
highlighted viewport.

Panning moves your view; it doesn't rearrange windows. Zooming out helps you see
more work at once. [Pan and zoom](PINCH_ZOOM.md) covers gesture cancellation,
window states, and device differences.

## Find your way back

**Alt+Tab** gathers open windows into a temporary grid. Keep Alt held and press
Tab to move forward, or Shift+Tab to move backward. Release Alt to choose; Escape
cancels. The arrangement you made remains intact.

**❖ + K** opens canvas search. Type a window title, app, file,
folder, or control name, then choose a result. File results come from your Desktop
directory, not a full-disk content index. Closing search restores the canvas.

You can also swipe four fingers sideways to switch windows, or pinch inward
with four fingers and lift to leave the switcher open. Touchscreens additionally
support five-finger switching. See [window switching](WINDOW_MANAGEMENT.md#window-switcher)
for the full interaction.

## Make the edges useful

Choose **Change layout** from Canvas tools at the bottom left, or press
**❖ + A**. Select a control, window, or desktop item. Drag it to an edge to keep
it within reach while the canvas moves, or choose Canvas or Background to put
it back with your work.

Drop frame attachments together to group them. Side controls turn vertically,
keeping their icons upright. Pinned windows leave a narrow visible strip until
you open them. Press Escape or Done when you're finished.

If you move or hide a control, search can find it again. Settings → System also
opens Change layout and restores Canvas tools. [Shell controls](SHELL.md)
explains removal, grouping, and narrow-screen behavior.

The full canvas-tool shortcut list is in **Settings → Keyboard shortcuts**.
These bindings are fixed for now. **❖ + O** opens the canvas map, and **❖ + G**
selects the window-group tool.

## Personalize the desktop

Open **Hearthspace Settings** from Apps List. Choose a light or dark appearance,
a coordinated accent, or a photo background. Displays controls arrangement and
scale. Choose the **Gestures** category in Settings to assign a simple drawing
to an app; the Draw tab in Apps List recognizes it when you lift your finger or
release the mouse.

A drawing that doesn't clearly match won't launch an app. This launcher doesn't
require a model. The separate AI opt-in supports gathering related windows and
explains its download before enabling it. See [gesture launching](FAVORITE_APPS.md)
and [window groups](WINDOW_GROUPS.md).

## Step away or finish

Power at the top right contains **Lock screen**, **Suspend**, **Log off**,
**Reboot**, and **Turn off**, subject to system availability. Ending the session
or shutting down asks for confirmation, with Cancel focused first.

**Super+L** locks a native session. Unlock locally with your account password.
The lock screen protects the running session; it doesn't close your apps.
Before logging off, save application work. Hearthspace remembers spatial
placement on normal shutdown, but it doesn't save documents on an app's behalf.

If something feels wrong, check the [known limits](ROADMAP.md) and report the
steps that led to it. The [documentation catalog](README.md) has detailed guides
for applications, notifications, clipboard, power, and other features.
