<div align="center">
  <img width="192" src="assets/logo.svg" alt="Hearthspace: a warm fire inside an open cube">
  <h1>Hearthspace</h1>
</div>

Hearthspace is a Linux desktop where your windows can extend beyond the edges
of your screen. You move around by panning and zooming, much like you would on
a map.

You might keep a document next to your notes and a browser, then move to another
part of the canvas for something else. When you come back, those windows are
still where you left them.

[![Watch Hearthspace in use](assets/demo-poster.png)](assets/demo.mp4)

## Finding your windows

Zoom out to see more at once, or use the canvas map to move to another area.
You can also search for a window or switch with **Alt+Tab**. Both let you choose
a window without changing the arrangement you made.

Some things are useful wherever you're working. Choose **Change layout** to
attach a window, the dock, or desktop controls to an edge of the screen. They'll
stay there as you move around the canvas.

## Try it

Hearthspace builds from source. Clone this repository and install the
[dependencies](docs/DEPENDENCIES.md), then run this from the repository folder:

```sh
scripts/install-session install
```

Run it as your normal user; it'll ask for administrator access when needed.
Once it's installed, save your work, log out, and select **Hearthspace** at the
login screen. The [installation guide](docs/LOGIN_SESSION.md) also covers updates
and removal.

To try it in a window inside your current desktop, follow the
[development setup](CONTRIBUTING.md#run-it). Some system controls are only
available when you log into Hearthspace directly.

Once you're in:

| Do this | With this |
| --- | --- |
| Open apps | Tap **Super**, or click **Apps List**. |
| Open a terminal | **Super+Return**. |
| Switch windows | **Alt+Tab**; add **Shift** to go backward. |
| Move around the canvas | Slide three fingers, or use **Super+arrow keys**. |
| Zoom | Pinch with three fingers, or use the map's zoom controls. |
| Change the layout | Choose **Change layout** in Canvas tools. |

The [first-session guide](docs/GETTING_STARTED.md) introduces the rest, including
mouse and keyboard navigation.

## Feedback and contributions

If something doesn't work as you expect, [open an issue](https://github.com/crutchcorn/hearthspace/issues).
Include what you were trying to do and what happened, along with the app's name
if one was involved.

For code, design, or documentation changes, see [CONTRIBUTING.md](CONTRIBUTING.md).
The [documentation](docs/README.md) has more on using and developing Hearthspace.

## License

The code is licensed under [Apache 2.0](LICENSE.md). The Hearthspace name and logo
aren't covered by that license; don't use them to imply endorsement of a
modified version, commercial product, or unrelated project without permission.
