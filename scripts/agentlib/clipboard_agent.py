"""Assert explicit agent Unicode clipboard ownership in actual editors."""

from .control import ControlError
from .input import Input


def run_agent_clipboard(run, *clients):
    text = "Agent paste: Café 日本語 Ελληνικά 🦀\nSecond line.\tEnd.\n" * 100
    for client in clients:
        run.reload(client, "")
        before = run.read(client)["paste_completed"]
        run.control.request("clipboard-set " + text.encode("utf-8").hex())
        with Input(run.control) as inputs:
            inputs.chord("ctrl+v")
        state = run.wait(
            client,
            "agent Unicode paste",
            lambda state, before=before: (
                state.get("paste_completed", -1) > before and state.get("text") == text
            ),
        )
        run.smoke.record(
            "agent-unicode-paste", backend=state["backend"], exact_text=True
        )
    for invalid in ("ff", "00", "61" * 16385):
        try:
            run.control.request("clipboard-set " + invalid)
        except ControlError:
            continue
        raise ControlError("invalid agent clipboard payload was accepted")
    # Invalid writes must preserve the last valid selection.
    client = clients[0]
    run.reload(client, "")
    with Input(run.control) as inputs:
        inputs.chord("ctrl+v")
    run.wait(
        client,
        "valid clipboard survives rejection",
        lambda state: state.get("text") == text,
    )
    run.smoke.record("agent-clipboard-rejection-preserves-selection", exact_text=True)
    run.smoke.capture("agent-unicode-paste")
