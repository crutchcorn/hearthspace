"""Exercise real CSD orchestration without starting input workers or native apps."""

import sys
import tempfile
import unittest
from contextlib import ExitStack, contextmanager
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from agentlib import gtk_titlebar
from agentlib.control import ControlError

TRACE = ("TOUCH_DOWN", "TOUCH_UP", "TOUCH_FRAME")
TITLEBAR = (
    ("button", False, "focused"),
    ("button-restore", False, "floating"),
    ("double-header", True, "canvas"),
    ("double-header-restore", True, "floating"),
    ("button-again", False, "focused"),
    ("double-header-from-focused", True, "canvas"),
    ("double-header-final-restore", True, "floating"),
)
KEYBOARD = (
    ("canvas-shortcut", "super+m", "canvas"),
    ("focused-shortcut", "super+shift+f", "focused"),
    ("focused-shortcut-exit", "super+shift+f", "canvas"),
    ("focused-again", "super+shift+f", "focused"),
    ("app-fullscreen", "f11", "fullscreen"),
    ("app-fullscreen-exit", "f11", "focused"),
    ("canvas-from-focused", "super+m", "canvas"),
    ("canvas-restore", "super+m", "floating"),
)


class GtkTitlebarLifecycleTests(unittest.TestCase):
    @contextmanager
    def harness(self, kind="gtk4", failure=None, missing=None, surviving=None):
        with tempfile.TemporaryDirectory() as directory, ExitStack() as patches:
            root = Path(directory)
            private, artifacts = root / "private-session", root / "artifacts"
            private.mkdir(mode=0o700)
            artifacts.mkdir()
            run = SimpleNamespace(
                devices=[],
                live=[],
                records=[],
                actions=[],
                checkpoints=[],
                setup=[],
                original={"canvas_x": 170, "canvas_y": 110},
                failure=RuntimeError("injected native boundary failure"),
            )
            smoke = run.smoke = SimpleNamespace(
                kind=kind,
                directory=private,
                artifacts=artifacts,
                target=17,
                gtk_count=40,
                fullscreen_entries=3,
                control=SimpleNamespace(deadline=object()),
            )
            smoke.control.request = lambda command: run.setup.append(
                ("request", command)
            )

            def record(event, **values):
                device = run.devices[-1]
                self.assertEqual(values["name"], device.name)
                if event == "csd-touchscreen-created":
                    self.assertEqual(values["device"], str(device.node))
                    self.assertEqual(run.live, [device])
                    self.assertTrue(device.node.exists())
                elif event == "csd-touchscreen-removed":
                    self.assertEqual(values["trace"], device.log.name)
                    self.assertTrue(device.log.closed)
                    self.assertFalse(device.node.exists())
                run.records.append((event, values["name"]))

            smoke.record = record

            @contextmanager
            def touch_device(session, log, device_kind="touchpad", deadline=None):
                self.assertIs(session, private)
                self.assertIs(deadline, smoke.control.deadline)
                self.assertEqual(device_kind, "touchscreen")
                self.assertFalse(run.live, "touchscreen workers must never overlap")
                self.assertEqual(run.setup[-1][0], "initial")
                self.assertEqual(Path(log.name).parent, artifacts)
                self.assertFalse(log.closed)
                node = private / f"event-{len(run.devices)}"
                device = SimpleNamespace(
                    name=Path(log.name).stem.removeprefix("libinput-"),
                    node=node,
                    log=log,
                    labels=[],
                    device={"device": str(node)},
                )
                run.devices.append(device)
                node.touch()
                run.live.append(device)
                log.write(f"created {device.name}\n")
                try:
                    yield device
                finally:
                    # Like the worker, retain diagnostic output while it exits;
                    # the caller owns closing the log and verifying node removal.
                    self.assertFalse(log.closed)
                    for event in TRACE:
                        if missing != (device.name, event):
                            log.write(f"{event}\n")
                    log.write(f"exited {device.name}\n")
                    if surviving != device.name:
                        node.unlink()
                        run.live.remove(device)

            def active(current, device):
                self.assertIs(current, smoke)
                if kind == "generic":
                    self.assertIsNone(device)
                    self.assertFalse(run.live)
                else:
                    self.assertEqual(run.live, [device])
                    self.assertTrue(device.node.exists())
                    self.assertFalse(device.log.closed)

            def fail(boundary, label):
                if failure == (boundary, label):
                    raise run.failure

            def gesture(current, device, label, header, touch):
                active(current, device)
                run.actions.append(
                    (
                        "gesture",
                        label,
                        header,
                        touch,
                        device,
                        smoke.gtk_count,
                        smoke.fullscreen_entries,
                    )
                )
                fail("gesture", label)

            def key(current, label, chord):
                device = run.live[0] if run.live else None
                active(current, device)
                run.actions.append(
                    (
                        "key",
                        label,
                        chord,
                        device,
                        smoke.gtk_count,
                        smoke.fullscreen_entries,
                    )
                )
                fail("key", label)

            def evidence(current, device, label, mode, original):
                active(current, device)
                self.assertIs(original, run.original)
                smoke.gtk_count += 1
                run.checkpoints.append(
                    (label, mode, device, smoke.gtk_count, smoke.fullscreen_entries)
                )
                if device is not None:
                    device.labels.append(label)
                    device.log.write(f"checkpoint {label}\n")
                fail("evidence", label)

            def initial(current, label, mode, original):
                self.assertIs(current, smoke)
                self.assertFalse(run.live)
                run.setup.append(("initial", label, mode, original))
                return run.original

            def viewport(current, **values):
                self.assertIs(current, smoke)
                run.setup.append(("viewport", values))

            for name, replacement in (
                ("TouchDevice", touch_device),
                ("gesture", gesture),
                ("key", key),
                ("evidence", evidence),
                ("wait_state", initial),
                ("set_viewport", viewport),
                ("settled", lambda current: run.setup.append(("settled",))),
            ):
                patches.enter_context(patch.object(gtk_titlebar, name, replacement))
            yield run

    def assert_logs(self, run, names, surviving=None):
        self.assertEqual([device.name for device in run.devices], names)
        expected_records = []
        for device in run.devices:
            path = run.smoke.artifacts / f"libinput-{device.name}.log"
            self.assertEqual(Path(device.log.name), path)
            self.assertTrue(device.log.closed)
            self.assertTrue(path.is_file())
            text = path.read_text()
            self.assertIn(f"created {device.name}\n", text)
            self.assertIn(f"exited {device.name}\n", text)
            for label in device.labels:
                self.assertIn(f"checkpoint {label}\n", text)
            self.assertEqual(device.node.exists(), device.name == surviving)
            expected_records.append(("csd-touchscreen-created", device.name))
            if device.name != surviving:
                expected_records.append(("csd-touchscreen-removed", device.name))
        self.assertEqual(run.records, expected_records)
        self.assertEqual(
            run.live, [device for device in run.devices if device.name == surviving]
        )

    def assert_sequence(self, run, scale, offset=0, device_offset=0):
        smoke = run.smoke
        prefix = f"{smoke.kind}-{scale}"
        generic = smoke.kind == "generic"
        expected_actions, expected_checkpoints = [], []
        entries = 3 + offset // (10 if generic else 22)

        def action(values, mode):
            expected_actions.append(
                (
                    *values,
                    40 + offset + len(expected_checkpoints),
                    entries + (mode == "fullscreen"),
                )
            )

        def checkpoint(name, mode, device):
            nonlocal entries
            entries += mode == "fullscreen"
            expected_checkpoints.append(
                (
                    f"{prefix}-{name}",
                    mode,
                    device,
                    40 + offset + len(expected_checkpoints) + 1,
                    entries,
                )
            )

        if generic:
            for name, mode in (("f9-maximize", "focused"), ("f9-restore", "floating")):
                action(("key", f"{prefix}-{name}", "f9", None), mode)
                checkpoint(name, mode, None)
            device = None
        else:
            for index, method in enumerate(("pointer", "touch")):
                device = run.devices[device_offset + index]
                for name, header, mode in TITLEBAR:
                    suffix = f"{method}-{name}"
                    action(
                        ("gesture", f"{prefix}-{suffix}", header, index == 1, device),
                        mode,
                    )
                    checkpoint(suffix, mode, device)
            device = run.devices[device_offset + 2]
        for name, chord, mode in KEYBOARD:
            action(("key", f"{prefix}-{name}", chord, device), mode)
            checkpoint(name, mode, device)
        count = 10 if generic else 22
        self.assertEqual(run.actions[offset : offset + count], expected_actions)
        self.assertEqual(run.checkpoints[offset : offset + count], expected_checkpoints)
        setup_offset = offset // count * 6
        self.assertEqual(
            run.setup[setup_offset : setup_offset + 6],
            [
                ("viewport", {"x": 0, "y": 0, "scale": 1}),
                ("request", "focus-window 17"),
                ("request", "move-window 17 170 110"),
                ("viewport", {"x": 0, "y": 0, "scale": scale}),
                ("settled",),
                ("initial", f"{prefix}-initial", "floating", None),
            ],
        )

    def test_each_gtk_scale_uses_three_fresh_devices_for_all_22_checkpoints(self):
        for kind in ("gtk3", "gtk4"):
            with self.subTest(kind=kind), self.harness(kind) as run:
                deadline = run.smoke.control.deadline
                for index, scale in enumerate((1, 0.8)):
                    gtk_titlebar.run(run.smoke, scale)
                    self.assert_sequence(run, scale, index * 22, index * 3)
                    self.assertIs(run.smoke.control.deadline, deadline)
                names = [
                    f"{kind}-{scale}-{batch}"
                    for scale in (1, 0.8)
                    for batch in ("pointer", "touch", "keyboard")
                ]
                self.assert_logs(run, names)
                self.assertEqual([len(d.labels) for d in run.devices], [7, 7, 8] * 2)
                self.assertEqual(len({id(d) for d in run.devices}), 6)
                self.assertEqual(len({d.node for d in run.devices}), 6)
                self.assertEqual(len(run.checkpoints), 44)
                self.assertEqual(
                    (run.smoke.gtk_count, run.smoke.fullscreen_entries), (84, 5)
                )
                for device in run.devices:
                    for event in TRACE:
                        self.assertIn(event, Path(device.log.name).read_text())

    def test_generic_keeps_ten_checkpoints_without_touchscreen_devices(self):
        with self.harness("generic") as run:
            for index, scale in enumerate((1, 0.8)):
                gtk_titlebar.run(run.smoke, scale)
                self.assert_sequence(run, scale, index * 10)
            self.assert_logs(run, [])
            self.assertEqual(list(run.smoke.artifacts.iterdir()), [])
            self.assertEqual(len(run.checkpoints), 20)
            self.assertEqual(
                (run.smoke.gtk_count, run.smoke.fullscreen_entries), (60, 5)
            )

    def test_boundary_failures_remove_active_device_and_skip_later_batches(self):
        cases = (
            (0, "pointer-button-restore", "gesture", 1),
            (1, "touch-button-restore", "gesture", 8),
            (2, "focused-shortcut", "key", 15),
        )
        names = [f"gtk4-0.8-{batch}" for batch in ("pointer", "touch", "keyboard")]
        for batch, suffix, action, checkpoint in cases:
            for boundary in (action, "evidence"):
                failure = (boundary, f"gtk4-0.8-{suffix}")
                with (
                    self.subTest(failure=failure),
                    self.harness(failure=failure) as run,
                ):
                    with self.assertRaises(RuntimeError) as raised:
                        gtk_titlebar.run(run.smoke, 0.8)
                    self.assertIs(raised.exception, run.failure)
                    self.assert_logs(run, names[: batch + 1])
                    self.assertEqual(len(run.actions), checkpoint + 1)
                    self.assertEqual(
                        len(run.checkpoints), checkpoint + (boundary == "evidence")
                    )

    def test_every_batch_requires_each_physical_touch_trace_event(self):
        names = [f"gtk4-0.8-{batch}" for batch in ("pointer", "touch", "keyboard")]
        for index, name in enumerate(names):
            for event in TRACE:
                with (
                    self.subTest(batch=name, missing=event),
                    self.harness(missing=(name, event)) as run,
                ):
                    with self.assertRaisesRegex(
                        ControlError, "lacks real touchscreen evidence"
                    ):
                        gtk_titlebar.run(run.smoke, 0.8)
                    self.assert_logs(run, names[: index + 1])
                    self.assertEqual(len(run.checkpoints), (7, 14, 22)[index])
                    self.assertNotIn(event, Path(run.devices[-1].log.name).read_text())

    def test_surviving_node_is_rejected_even_when_batch_already_failed(self):
        names = [f"gtk4-0.8-{batch}" for batch in ("pointer", "touch", "keyboard")]
        for index, suffix in enumerate(
            ("pointer-button", "touch-button", "canvas-shortcut")
        ):
            for boundary in (None, "evidence"):
                failure = (boundary, f"gtk4-0.8-{suffix}") if boundary else None
                with (
                    self.subTest(batch=names[index], failure=failure),
                    self.harness(failure=failure, surviving=names[index]) as run,
                ):
                    with self.assertRaisesRegex(ControlError, "survived cleanup"):
                        gtk_titlebar.run(run.smoke, 0.8)
                    self.assert_logs(run, names[: index + 1], surviving=names[index])
                    expected = (0, 7, 14)[index] + 1 if boundary else (7, 14, 22)[index]
                    self.assertEqual(len(run.checkpoints), expected)


if __name__ == "__main__":
    unittest.main()
