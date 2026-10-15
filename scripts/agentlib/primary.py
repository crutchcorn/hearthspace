"""Exact middle-click primary-selection checks, independent of Ctrl+C clipboard."""

from .input import Input


def run_primary(run, wayland, x11):
    sentinel = "The explicit clipboard survives primary selection changes."
    run.transfer(wayland, x11, sentinel, "primary-clipboard-sentinel", capture=False)
    for source, target, label in (
        (wayland, x11, "primary-wayland-to-x11"),
        (x11, wayland, "primary-x11-to-wayland"),
    ):
        payload = f"{label}: Café 日本語 🦀\nSecond line."
        run.reload(target, "")
        before = run.reload(source, payload)["primary_revision"]
        with Input(run.control) as inputs:
            inputs.chord("ctrl+a")
        run.wait(
            source,
            "primary selection owner",
            lambda state, before=before: state.get("primary_revision", -1) > before,
        )
        run.focus(target)
        window = run.window(target)
        with Input(run.control) as inputs:
            inputs.click(
                window["x"] + 70,
                window["y"] + window["height"] / 2,
                button="middle",
            )
        run.wait(
            target, label, lambda state, payload=payload: state.get("text") == payload
        )
        run.smoke.record(label, exact_text=True, bytes=len(payload.encode("utf-8")))
        run.smoke.capture(label)

        run.reload(target, "")
        with Input(run.control) as inputs:
            inputs.chord("ctrl+v")
        run.wait(
            target,
            "clipboard remains independent",
            lambda state: state.get("text") == sentinel,
        )
        run.smoke.record(f"{label}-clipboard-independent", exact_text=True)
