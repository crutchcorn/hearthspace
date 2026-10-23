"""Desktop workflow shared by isolated headless sessions and native review."""

import json
import time

from .control import ControlError
from .daily_checks import a11y
from .input import Input
from .viewport_fixture import set_viewport


def desktop(smoke):
    return smoke.control.query("desktop-status")


def item(smoke, identity):
    return next(
        (entry for entry in desktop(smoke)["items"] if entry["id"] == identity), None
    )


def selected(smoke):
    return {entry["id"] for entry in desktop(smoke)["items"] if entry["selected"]}


def point(smoke, entry):
    view = smoke.control.status()["viewport"]
    return (
        (entry["x"] + 44 - view["x"]) * view["scale"],
        (entry["y"] + 48 - view["y"]) * view["scale"],
    )


def position(entry):
    return entry["x"], entry["y"]


def click(smoke, identity, modifier=None):
    with Input(smoke.control) as inputs:
        if modifier:
            inputs.down("key", modifier)
        inputs.click(*point(smoke, item(smoke, identity)))


def drag(smoke, identity, dx, dy, modifier=None):
    x, y = point(smoke, item(smoke, identity))
    with Input(smoke.control) as inputs:
        if modifier:
            inputs.down("key", modifier)
        inputs.drag(x, y, x + dx, y + dy)


def require(condition, message):
    if not condition:
        raise ControlError(message)


def opened(smoke, title):
    window = smoke.until(
        f"opened desktop {title}",
        lambda: next(
            (
                w
                for w in smoke.control.status()["windows"]
                if w["title"] == title and w["mapped"]
            ),
            None,
        ),
    )
    require(window["focused"], "desktop activation did not focus the application")
    smoke.control.request(f"close-window {window['id']}")
    smoke.until("desktop app closed", lambda: smoke.window(window["id"]) is None)


def run(smoke):
    first, second = "app:a-terminal.desktop", "app:b-editor.desktop"
    smoke.until(
        "installed apps and files mapped", lambda: len(desktop(smoke)["items"]) == 5
    )
    before = desktop(smoke)
    require(
        {entry["id"] for entry in before["items"]} >= {first, second},
        "catalog shortcut missing",
    )
    require(
        not smoke.control.status()["windows"],
        "desktop icons leaked into app window list",
    )
    smoke.capture("desktop-initial")

    with Input(smoke.control) as inputs:
        inputs.drag(10, 100, 230, 530, duration=0)
    require(
        len(selected(smoke)) == 5, "marquee did not select intersecting desktop items"
    )
    smoke.capture("desktop-marquee-selection")
    with Input(smoke.control) as inputs:
        inputs.chord("escape")

    click(smoke, first)
    smoke.until("single click selects", lambda: selected(smoke) == {first})
    require(not smoke.control.status()["windows"], "single click launched an app")
    require(
        not item(smoke, first)["keyboard_focus"],
        "pointer selection showed keyboard focus",
    )
    click(smoke, second, 29)
    smoke.until("Ctrl adds selection", lambda: selected(smoke) == {first, second})
    origin = {identity: position(item(smoke, identity)) for identity in [first, second]}
    drag(smoke, first, 120, 60)
    for identity in [first, second]:
        require(
            position(item(smoke, identity))
            == (origin[identity][0] + 120, origin[identity][1] + 60),
            "group drag did not preserve item spacing",
        )
    require(not smoke.control.status()["windows"], "drag activated an app")
    smoke.capture("desktop-group-drag")

    origin = {identity: position(item(smoke, identity)) for identity in [first, second]}
    x, y = point(smoke, item(smoke, first))
    with Input(smoke.control) as inputs:
        inputs.motion(x, y)
        inputs.down("pointer-button", 272)
        inputs.motion(x + 80, y + 80)
        inputs.chord("escape")
        inputs.up("pointer-button", 272)
    for identity in [first, second]:
        require(
            position(item(smoke, identity)) == origin[identity],
            "Escape did not restore desktop placement",
        )

    click(smoke, first)
    drag(smoke, first, 19, 31, 56)
    snapped = item(smoke, first)
    require(
        (snapped["x"] - 14) % 28 == 0 and (snapped["y"] - 14) % 28 == 0,
        "Alt drag did not align icon to dot grid",
    )
    smoke.capture("desktop-snapped")
    with Input(smoke.control) as inputs:
        inputs.chord("tab")
    focused = smoke.until(
        "Tab focuses desktop icon",
        lambda: next(
            (
                entry
                for entry in desktop(smoke)["items"]
                if entry["focused"] and entry["keyboard_focus"]
            ),
            None,
        ),
    )
    # Native accessibility exposes the same selected state and focus, even
    # though the compositor owns cross-surface Tab traversal.
    smoke.until(
        "native desktop control focus",
        lambda: a11y(smoke, "find", "//*[@role='toggle button' and @focused='true']")[
            "elements"
        ],
    )
    smoke.capture("desktop-keyboard-focus")
    with Input(smoke.control) as inputs:
        inputs.chord("space")
    smoke.until(
        "Space selects focused icon", lambda: selected(smoke) == {focused["id"]}
    )

    click(smoke, first)
    # A deliberate gap keeps this keyboard activation independent of doubleclick.
    with Input(smoke.control) as inputs:
        inputs.chord("enter")
    opened(smoke, "DESKTOP_TERMINAL")
    time.sleep(0.4)
    with Input(smoke.control) as inputs:
        inputs.click(*point(smoke, item(smoke, second)))
        inputs.click(*point(smoke, item(smoke, second)))
    opened(smoke, "DESKTOP_EDITOR")

    display = smoke.until(
        "XWayland ready for default application",
        lambda: smoke.control.status().get("xwayland_display"),
    )
    for name in ["Notes.txt", "Pictures"]:
        click(smoke, f"file:{name.encode().hex()}")
        with Input(smoke.control) as inputs:
            inputs.chord("enter")
        opened(smoke, "DESKTOP_FILE")
    require(
        [
            json.loads(line)
            for line in smoke.service_fixture.opened_paths.read_text().splitlines()
        ]
        == [
            {
                "paths": [str(smoke.service_fixture.desktop / name)],
                "display": display,
            }
            for name in ["Notes.txt", "Pictures"]
        ],
        "default application did not receive the exact desktop paths and session display",
    )
    require(
        json.loads(smoke.service_fixture.x11_state.read_text())["backend"] == "x11",
        "folder default application did not open through XWayland",
    )

    original = position(item(smoke, first))
    set_viewport(smoke, x=50, y=30, scale=1.5)
    click(smoke, first)
    require(selected(smoke) == {first}, "zoomed desktop hit testing missed the icon")
    drag(smoke, first, 90, 60)
    require(
        position(item(smoke, first)) == (original[0] + 60, original[1] + 40),
        "zoomed movement did not use canvas coordinates",
    )
    set_viewport(smoke, x=0, y=0, scale=1)
    smoke.capture("desktop-after-zoom")

    original = position(item(smoke, first))
    x, y = point(smoke, item(smoke, first))
    with Input(smoke.control) as inputs:
        inputs.motion(x, y)
        inputs.down("pointer-button", 272)
        inputs.motion(1278, 400)
        smoke.until(
            "desktop drag pans at edge",
            lambda: smoke.control.status()["viewport"]["x"] > 20,
        )
        inputs.chord("escape")
    require(
        position(item(smoke, first)) == original, "edge-pan cancellation moved icon"
    )
    set_viewport(smoke, x=0, y=0, scale=1)

    click(smoke, first)
    origin = position(item(smoke, first))
    apps = next(
        corner
        for corner in smoke.control.status()["shell_corners"]
        if corner["corner"] == "apps"
    )
    x, y = point(smoke, item(smoke, first))
    with Input(smoke.control) as inputs:
        inputs.drag(x, y, apps["x"] + apps["width"] - 27, apps["y"] + 27)
    favorites = smoke.service_fixture.favorites
    smoke.until(
        "desktop favorite saved",
        lambda: (
            favorites.is_file()
            and any(
                favorite["app_id"] == "a-terminal.desktop"
                for favorite in json.loads(favorites.read_text())["favorites"]
            )
        ),
    )
    require(
        position(item(smoke, first)) == origin,
        "favoriting moved or consumed desktop shortcut",
    )
    require(
        not smoke.control.status()["windows"], "favorite drop also launched the app"
    )
    smoke.capture("desktop-favorite-keeps-shortcut")

    # Removing a personal file updates only that item, retaining app identities
    # and user placements through catalog reconciliation.
    previous = position(item(smoke, first))
    (smoke.service_fixture.desktop / "Notes.txt").unlink()
    smoke.until("removed file disappears", lambda: len(desktop(smoke)["items"]) == 4)
    require(
        position(item(smoke, first)) == previous, "catalog refresh moved app shortcut"
    )
    smoke.record(
        "desktop-verified",
        selection=True,
        group_drag=True,
        cancel=True,
        keyboard=True,
        activation=True,
        file_and_folder_open=True,
        x11_default_app=True,
        zoom=True,
        edge_pan=True,
        live_directory=True,
        favorite_drop=True,
        marquee=True,
    )
    positions = {
        entry["id"]: {"x": entry["x"], "y": entry["y"]}
        for entry in desktop(smoke)["items"]
    }
    smoke.control.request("quit")
    saved = smoke.service_fixture.config / "desktop-layout.json"
    smoke.until("desktop positions persisted", lambda: saved.is_file())
    persisted = json.loads(saved.read_text())["desktop"]
    require(
        all(
            persisted[identity] == position for identity, position in positions.items()
        ),
        "shutdown lost desktop positions",
    )
