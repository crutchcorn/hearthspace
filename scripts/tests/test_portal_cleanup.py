import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import portal_cleanup

RUNTIME = Path("/run/user/1000/private")
HOST = (
    "36 25 0:32 / /run/user/1000/doc rw - fuse.portal portal rw\n"
    "38 25 0:34 / /run/user/1000/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw\n"
)
DOCUMENT = "37 25 0:33 / /run/user/1000/private/doc rw - fuse.portal portal rw\n"
GVFS = "39 25 0:35 / /run/user/1000/private/gvfs rw - fuse.gvfsd-fuse gvfsd-fuse rw\n"


class PortalCleanupTests(unittest.TestCase):
    def test_only_mounts_beneath_exact_owned_runtime_are_selected(self):
        self.assertEqual(
            portal_cleanup.runtime_mounts(HOST + DOCUMENT + GVFS, RUNTIME),
            [(RUNTIME / "doc", "fuse.portal"), (RUNTIME / "gvfs", "fuse.gvfsd-fuse")],
        )
        self.assertEqual(
            portal_cleanup.runtime_mounts(DOCUMENT, Path("/run/user/1000/priv")), []
        )

    def test_spaces_in_owned_mount_paths_are_decoded(self):
        mounts = (
            "37 25 0:33 / /tmp/private\\040session/doc rw - fuse.portal portal rw\n"
        )
        self.assertEqual(
            portal_cleanup.runtime_mounts(mounts, Path("/tmp/private session")),
            [(Path("/tmp/private session/doc"), "fuse.portal")],
        )

    def test_cleanup_never_runs_for_host_mounts(self):
        with (
            patch.object(Path, "read_text", return_value=HOST),
            patch.object(portal_cleanup, "run") as run,
        ):
            portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)
        run.assert_not_called()

    def test_cleanup_detaches_both_owned_mounts_and_preserves_host(self):
        with (
            patch.object(Path, "read_text", side_effect=[HOST + DOCUMENT + GVFS, HOST]),
            patch.object(portal_cleanup, "run") as run,
        ):
            portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)
        self.assertEqual(
            [call.args[0] for call in run.call_args_list],
            [
                ["fusermount3", "-u", "-z", "--", RUNTIME / "doc"],
                ["fusermount3", "-u", "-z", "--", RUNTIME / "gvfs"],
            ],
        )

    def test_wrong_filesystem_and_unknown_mounts_are_never_detached(self):
        for unexpected in [
            GVFS.replace("fuse.gvfsd-fuse", "ext4"),
            GVFS.replace("/private/gvfs", "/private/other"),
            GVFS.replace("/private/gvfs", "/private/gvfs/nested"),
            GVFS.replace("/private/gvfs", "/private"),
        ]:
            with (
                self.subTest(unexpected=unexpected),
                patch.object(Path, "read_text", return_value=DOCUMENT + unexpected),
                patch.object(portal_cleanup, "run") as run,
                self.assertRaisesRegex(RuntimeError, "unexpected mounted"),
            ):
                portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)
            run.assert_not_called()

    def test_remaining_mount_prevents_recursive_runtime_cleanup(self):
        with (
            patch.object(Path, "read_text", return_value=GVFS),
            patch.object(portal_cleanup, "run"),
            self.assertRaisesRegex(RuntimeError, "still contains mounted"),
        ):
            portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)

    def test_mounts_share_one_deadline(self):
        with (
            patch.object(Path, "read_text", return_value=DOCUMENT + GVFS),
            patch.object(
                portal_cleanup.time, "monotonic", side_effect=[10, 10.2, 11.1]
            ),
            patch.object(portal_cleanup, "run") as run,
            self.assertRaises(TimeoutError),
        ):
            portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)
        self.assertEqual(run.call_count, 1)
        self.assertAlmostEqual(run.call_args.kwargs["timeout"], 0.8)

    def test_symlink_runtime_is_rejected(self):
        with (
            patch.object(Path, "is_symlink", return_value=True),
            patch.object(portal_cleanup, "run") as run,
            self.assertRaisesRegex(RuntimeError, "symlink"),
        ):
            portal_cleanup.cleanup_runtime_mounts(RUNTIME, 1)
        run.assert_not_called()

    def test_malformed_mountinfo_does_not_crash(self):
        self.assertEqual(portal_cleanup.runtime_mounts("invalid - \n", RUNTIME), [])


if __name__ == "__main__":
    unittest.main()
