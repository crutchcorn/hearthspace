"""Keep movement evidence contrasted through each owned GTK client's lifetime."""

import sys
import tempfile
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import gtk_touch
from agentlib.control import ControlError, Deadline


class GtkTouchBackdropTests(unittest.TestCase):
    @contextmanager
    def harness(self, *, titlebar=False, inspect=False, fail=False):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as patches:
            args = SimpleNamespace(
                session=Path(directory),
                titlebar_maximize=False,
                inspect_only=inspect,
                case="all",
                timeout=30,
                total_timeout=90,
            )
            smoke = gtk_touch.GtkTouchSmoke(args)
            # Only the workflow is exercised: session/app/device I/O is replaced
            # at its boundary. The real step, ready, and client methods run.
            args.titlebar_maximize = titlebar
            output = {"logical_width": 1920, "logical_height": 1080}
            run = SimpleNamespace(events=[], live=False, clients=[], smoke=smoke)
            view = {"x": 20, "y": 40, "scale": 0.5, "animating": False}
            control = Mock(deadline=Deadline(30))
            control.status.return_value = {
                "backend": "native",
                "shell_ready": True,
                "outputs": [output],
                "viewport": view,
            }

            def request(command):
                parts = command.split()
                if parts[0] == "zoom-to":
                    view["scale"] = float(parts[1])
                elif parts[0] == "pan-to":
                    view.update(x=float(parts[1]), y=float(parts[2]))
                else:
                    self.fail(f"unexpected fixture command: {command}")

            control.request.side_effect = request
            smoke.control = control
            smoke.connect = lambda: {"command_socket": "/unused-owned-socket"}
            smoke.record = Mock()
            smoke.capture = Mock()
            smoke.failure_evidence = lambda name: run.events.append(
                ("failure", name, run.live, tuple(run.clients))
            )
            patches.enter_context(patch("agentlib.daily.Control", return_value=control))
            patches.enter_context(patch.object(gtk_touch, "a11y", return_value={}))

            @contextmanager
            def contrast(owner, target):
                self.assertIs(owner, smoke)
                self.assertIs(target, output)
                self.assertEqual(view, {"x": 0, "y": 0, "scale": 1, "animating": False})
                self.assertFalse(run.clients)
                run.live = True
                run.events.append("backdrop-open")
                try:
                    yield
                finally:
                    self.assertFalse(run.clients)
                    run.events.append("backdrop-close")
                    run.live = False

            @contextmanager
            def application(owner, kind):
                self.assertIs(owner, smoke)
                self.assertEqual(run.live, not (titlebar or inspect))
                run.clients.append(kind)
                run.events.append(("app-open", kind))
                try:
                    yield
                finally:
                    run.events.append(("app-close", kind))
                    run.clients.remove(kind)

            def interact(owner, scale):
                self.assertIs(owner, smoke)
                self.assertEqual(run.live, not titlebar)
                run.events.append(("input", run.clients[0], scale))
                if fail:
                    raise ControlError("owned input failed")

            patches.enter_context(patch.object(gtk_touch, "backdrop", contrast))
            patches.enter_context(patch.object(gtk_touch, "application", application))
            patches.enter_context(patch.object(gtk_touch.checks, "run", interact))
            patches.enter_context(patch.object(gtk_touch.gtk_titlebar, "run", interact))
            yield run

    def test_movement_backdrop_survives_both_apps_and_both_scales(self):
        with self.harness() as run:
            run.smoke.workflow()
            self.assertEqual(
                run.events,
                [
                    "backdrop-open",
                    ("app-open", "calculator"),
                    ("input", "calculator", 1),
                    ("input", "calculator", 0.8),
                    ("app-close", "calculator"),
                    ("app-open", "gtk3"),
                    ("input", "gtk3", 1),
                    ("input", "gtk3", 0.8),
                    ("app-close", "gtk3"),
                    "backdrop-close",
                ],
            )
            self.assertFalse(run.live)

    def test_failure_captures_client_before_app_and_backdrop_cleanup(self):
        with self.harness(fail=True) as run:
            with self.assertRaisesRegex(ControlError, "owned input failed"):
                run.smoke.workflow()
            self.assertEqual(
                run.events,
                [
                    "backdrop-open",
                    ("app-open", "calculator"),
                    ("input", "calculator", 1),
                    ("failure", "calculator-failed", True, ("calculator",)),
                    ("app-close", "calculator"),
                    "backdrop-close",
                ],
            )
            self.assertFalse(run.live)

    def test_titlebar_matrix_keeps_its_existing_presentation_setup(self):
        with self.harness(titlebar=True) as run:
            run.smoke.workflow()
            self.assertNotIn("backdrop-open", run.events)
            self.assertEqual(
                [event for event in run.events if event[0] == "input"],
                [
                    ("input", "gtk3", 1),
                    ("input", "gtk3", 0.8),
                    ("input", "gtk4", 1),
                    ("input", "gtk4", 0.8),
                    ("input", "generic", 1),
                    ("input", "generic", 0.8),
                ],
            )

    def test_inspection_does_not_add_a_backdrop_or_inject_input(self):
        with self.harness(inspect=True) as run:
            run.smoke.workflow()
            self.assertEqual(
                run.events,
                [
                    ("app-open", "calculator"),
                    ("app-close", "calculator"),
                    ("app-open", "gtk3"),
                    ("app-close", "gtk3"),
                ],
            )
            run.smoke.control.request.assert_not_called()


if __name__ == "__main__":
    unittest.main()
