"""Do not complete an installation with an incompatible sharing backend."""

import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib.portal import BACKENDS, compatible, validate_backend


class PortalDependencyTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name)

    def backend(self, name, version=None):
        path = self.directory / name
        path.write_text("#!/bin/sh\nexit 9\n")
        path.chmod(0o755)
        if version is not None:
            path.with_name(path.name + ".json").write_text(
                json.dumps(
                    {
                        "version": version,
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                    }
                )
            )
        return path

    def test_old_unversioned_and_prerelease_backend_rejected_without_launch(self):
        for version in (None, "0.7.1", "0.8.1-1", "0.8.3", "0.8.4~rc1"):
            with self.subTest(version=version):
                path = self.backend(str(version), version)
                with self.assertRaisesRegex(RuntimeError, "requires a verified"):
                    validate_backend((path,))

    def test_verified_source_build_receipt_accepts_matching_binary(self):
        path = self.backend("current", "0.8.4")
        self.assertEqual(validate_backend((path,)), path)
        path.write_text("#!/bin/sh\nexit 0\n")
        with self.assertRaisesRegex(RuntimeError, "requires a verified"):
            validate_backend((path,))

    def test_obsolete_local_binary_cannot_be_masked_by_compatible_system_binary(self):
        old = self.backend("local", "0.8.1")
        new = self.backend("system", "0.8.4")
        with self.assertRaisesRegex(RuntimeError, "requires a verified"):
            validate_backend((old, new))

    def test_package_version_checked_without_starting_backend(self):
        original_access = os.access
        for version, accepted in (("0.8.1-1", False), ("0.8.4-1ubuntu1", True)):
            with self.subTest(version=version):
                result = subprocess.CompletedProcess(
                    ["dpkg-query"], 0, f"install ok installed\n{version}", ""
                )
                with (
                    patch(
                        "installlib.portal.os.access",
                        side_effect=lambda path, mode: (
                            path == BACKENDS[1] or original_access(path, mode)
                        ),
                    ),
                    patch("installlib.portal.Path.is_file", return_value=False),
                    patch(
                        "installlib.portal.subprocess.run", return_value=result
                    ) as run,
                ):
                    if accepted:
                        self.assertEqual(validate_backend((BACKENDS[1],)), BACKENDS[1])
                    else:
                        with self.assertRaises(RuntimeError):
                            validate_backend((BACKENDS[1],))
                    self.assertEqual(run.call_args.args[0][0], "dpkg-query")

    def test_distribution_versions_and_future_releases(self):
        for version in ("0.8.4", "1:0.8.4-1", "0.8.5+git123", "1.0.0"):
            self.assertTrue(compatible(version), version)
        for version in ("", "garbage", "0.8.3", "0.8.4~rc1"):
            self.assertFalse(compatible(version), version)


if __name__ == "__main__":
    unittest.main()
