"""Bounded, read-only observation of a user's local swaylock authentication."""

import math
import time

from . import power_evidence as verify
from .control import Control, ControlError, Deadline

WORKFLOW_BUDGET = 240
DEFAULT_UNLOCK_TIMEOUT = 300


def configure(args):
    manual = getattr(args, "manual_unlock", False)
    unlock = getattr(args, "unlock_timeout", None)
    if manual and getattr(args, "locker", "fixture") != "swaylock":
        raise ControlError("--manual-unlock requires --locker swaylock")
    if not manual and unlock is not None:
        raise ControlError("--unlock-timeout requires --manual-unlock")
    if manual:
        unlock = DEFAULT_UNLOCK_TIMEOUT if unlock is None else unlock
        if not math.isfinite(unlock) or not 1 <= unlock <= 900:
            raise ControlError(
                "manual unlock timeout must be between 1 and 900 seconds"
            )
    minimum = WORKFLOW_BUDGET + (unlock if manual else 0)
    total = getattr(args, "total_timeout", None)
    total = minimum if total is None else total
    if manual and (not math.isfinite(total) or total < minimum):
        raise ControlError(
            "--total-timeout must reserve --unlock-timeout plus 240 seconds "
            "for the suspend and desktop checks"
        )
    args.manual_unlock, args.unlock_timeout, args.total_timeout = manual, unlock, total


def observe(smoke):
    """Never send input or an unlock request, even when the owner disappears."""
    available = smoke.total.remaining() - smoke.args.timeout
    if available <= 0:
        raise ControlError(
            "insufficient total budget for manual unlock and desktop checks"
        )
    budget = min(smoke.args.unlock_timeout, available)
    deadline = Deadline(budget)
    started = time.monotonic()
    compositor = smoke.processes_before["compositor"]
    expected = smoke.owned_lockers
    if len(expected) != 1:
        raise ControlError(
            "manual unlock requires exactly one confirmed owned swaylock"
        )

    def remaining():
        try:
            available = smoke.total.remaining() - smoke.args.timeout
            if available <= 0:
                raise ControlError("desktop verification budget is reserved")
            return min(deadline.remaining(), available)
        except ControlError as error:
            raise ControlError("timed out waiting for local swaylock unlock") from error

    def sample():
        # Each status exchange is short, even though a human may take minutes.
        smoke.control = Control(
            smoke.manifest["command_socket"],
            Deadline(min(2.0, smoke.args.timeout, remaining())),
        )
        if verify.process_identity(compositor["pid"]) != compositor:
            raise ControlError("compositor changed while waiting for manual unlock")
        status = smoke.control.status()
        lockers = verify.owned_swaylocks(compositor["pid"])
        if any(locker != expected[0] for locker in lockers):
            raise ControlError("owned swaylock changed during manual unlock")
        remaining()
        return status, lockers

    initial, lockers = sample()
    verify.locked(initial)
    verify.recovery_ready(initial)
    if lockers != expected:
        raise ControlError("confirmed swaylock exited before the manual unlock wait")
    smoke.record("step-start", name="manual-unlock", budget=budget)
    smoke.record(
        "manual-unlock-wait",
        lockers=lockers,
        timeout_seconds=budget,
        instructions=(
            "Type your password on this machine and press Enter. Do not send it "
            "to the agent. After unlocking, leave the devices idle during checks."
        ),
    )
    while True:
        status, lockers = sample()
        if status.get("locked") is True:
            verify.locked(status)
        elif status.get("locked") is False:
            if (
                status.get("lock_confirmed") is not False
                or status.get("active") is not True
            ):
                raise ControlError("manual unlock did not restore the active desktop")
            verify.recovery_ready(status)
            if not lockers:
                smoke.preserved_processes()
                remaining()
                smoke.record(
                    "manual-unlock-observed",
                    status=status,
                    locker=expected[0],
                    password_supplied_by_harness=False,
                )
                smoke.record(
                    "step-passed",
                    name="manual-unlock",
                    elapsed=time.monotonic() - started,
                )
                return
        else:
            raise ControlError("manual unlock status did not include a lock state")
        # A disappearing owner while still locked is not evidence of unlock.
        # Allow the pending protocol request to dispatch, but never replace it.
        time.sleep(min(0.2, remaining()))
