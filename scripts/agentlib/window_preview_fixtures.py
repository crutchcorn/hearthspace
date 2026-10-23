"""Terminal fixtures with distinct client pixels for live window previews."""

from .firefox_assertions import fullscreen_at_output
from .input import Input
from .window_preview_assertions import COLORS, marker_count


def fixtures(smoke):
    smoke.targets = {}
    for index, color in enumerate(COLORS):
        old = {item["id"] for item in smoke.control.status()["windows"]}
        smoke.control.request("spawn foot")
        window = smoke.until(
            "new terminal mapped and focused",
            lambda old=old: next(
                (
                    item
                    for item in smoke.control.status()["windows"]
                    if item["id"] not in old and item["mapped"] and item["focused"]
                ),
                None,
            ),
        )
        target = window["id"]
        smoke.targets[target] = color
        title = f"{smoke.marker}_{index}"
        marker = (" " * 24 + r"\n") * 5
        with Input(smoke.control) as inputs:
            inputs.type("exec bash --noprofile --norc\n")
            inputs.type(
                "export PS1='$ '; unset PROMPT_COMMAND; "
                f"printf '\\033]0;{title}\\007\\033[2J\\033[H"
                f"\\033[48;2;{color[0]};{color[1]};{color[2]}m{marker}\\033[0m\\n'\n"
            )
        smoke.until(
            "unique terminal title",
            lambda target=target, title=title: smoke.window(target)["title"] == title,
        )

        def visible(target=target, color=color):
            image = smoke.control.capture()
            item = smoke.window(target)
            bounds = (
                item["x"],
                item["y"],
                item["x"] + item["width"],
                item["y"] + item["height"],
            )
            return marker_count(image, bounds, color) >= 100

        smoke.until("terminal color marker presented", visible)
        smoke.capture(f"fixture-{index}")
    first, second, third = smoke.targets
    smoke.control.request(f"move-window {first} 180 160")
    smoke.control.request(f"move-window {second} 220 190")
    smoke.control.request(f"move-window {third} 100000 100000")
    smoke.control.request(f"fullscreen-window {first} true")
    smoke.until(
        "fullscreen client committed",
        lambda: fullscreen_at_output(smoke.window(first), [smoke.output]),
    )
    smoke.capture("fixtures-arranged")
    smoke.record(
        "fixtures-arranged-state", targets=smoke.targets, status=smoke.control.status()
    )
