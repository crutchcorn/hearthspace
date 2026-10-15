"""Explicit viewport placement for isolated test fixtures."""


def set_viewport(smoke, *, x, y, scale):
    # Wait between independent operations: pan-to cancels an in-flight zoom.
    smoke.control.request(f"zoom-to {scale}")
    smoke.until(
        "fixture zoom settled",
        lambda: (
            not (view := smoke.control.status()["viewport"]).get("animating", False)
            and abs(view["scale"] - scale) < 1e-8
        ),
    )
    smoke.control.request(f"pan-to {x} {y}")
    smoke.until(
        "fixture canvas position",
        lambda: (
            (view := smoke.control.status()["viewport"])["x"] == x and view["y"] == y
        ),
    )
