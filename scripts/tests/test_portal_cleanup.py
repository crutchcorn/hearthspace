import sys
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nativelib import portal_cleanup


class PortalCleanupTests(unittest.TestCase):
    def test_only_exact_owned_document_mount_is_selected(self):
        mounts = (
            "36 25 0:32 / /run/user/1000/doc rw - fuse.portal portal rw\n"
            "37 25 0:33 / /run/user/1000/private/doc rw - fuse.portal portal rw\n"
        )
        self.assertTrue(
            portal_cleanup.document_mounted(mounts, Path("/run/user/1000/private/doc"))
        )
        self.assertFalse(
            portal_cleanup.document_mounted(mounts, Path("/run/user/1000/other/doc"))
        )
        self.assertFalse(
            portal_cleanup.document_mounted(
                mounts.replace("fuse.portal", "ext4"),
                Path("/run/user/1000/private/doc"),
            )
        )

    def test_spaces_in_owned_mount_paths_are_decoded(self):
        mounts = (
            "37 25 0:33 / /tmp/private\\040session/doc rw - fuse.portal portal rw\n"
        )
        self.assertTrue(
            portal_cleanup.document_mounted(mounts, Path("/tmp/private session/doc"))
        )
        self.assertFalse(
            portal_cleanup.document_mounted(mounts, Path("/tmp/private session"))
        )

    def test_cleanup_never_runs_for_only_the_host_portal(self):
        mounts = "36 25 0:32 / /run/user/1000/doc rw - fuse.portal portal rw\n"
        with (
            patch.object(Path, "read_text", return_value=mounts),
            patch.object(portal_cleanup, "run") as run,
        ):
            portal_cleanup.cleanup_documents(Path("/run/user/1000/private"), 1)
        run.assert_not_called()

    def test_cleanup_verifies_the_owned_mount_disappears(self):
        mounts = "37 25 0:33 / /run/user/1000/private/doc rw - fuse.portal portal rw\n"
        with (
            patch.object(Path, "read_text", side_effect=[mounts, ""]),
            patch.object(portal_cleanup, "run") as run,
        ):
            portal_cleanup.cleanup_documents(Path("/run/user/1000/private"), 1)
        self.assertEqual(
            run.call_args.args[0],
            ["fusermount3", "-u", "-z", "--", Path("/run/user/1000/private/doc")],
        )


if __name__ == "__main__":
    unittest.main()
