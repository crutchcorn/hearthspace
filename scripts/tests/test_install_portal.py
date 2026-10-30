"""Do not complete an installation with an incompatible sharing backend."""

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from installlib.portal import (
    LIFECYCLE_PATCH,
    POLICY_PATCH,
    REVISION,
    START_PATCH,
    patch_digest,
    validate_backend,
)


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
                        "source_revision": REVISION,
                        "session_policy_sha256": patch_digest(POLICY_PATCH),
                        "start_session_sha256": patch_digest(START_PATCH),
                        "capture_lifecycle_sha256": patch_digest(LIFECYCLE_PATCH),
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

    def test_unpatched_or_different_source_receipt_is_rejected(self):
        path = self.backend("policy", "0.8.4")
        receipt = path.with_name(path.name + ".json")
        correct = json.loads(receipt.read_text())
        for key in (
            "source_revision",
            "session_policy_sha256",
            "start_session_sha256",
            "capture_lifecycle_sha256",
        ):
            for replacement in (None, "incorrect"):
                with self.subTest(key=key, replacement=replacement):
                    data = correct.copy()
                    if replacement is None:
                        del data[key]
                    else:
                        data[key] = replacement
                    receipt.write_text(json.dumps(data))
                    with self.assertRaisesRegex(RuntimeError, "session-policy patch"):
                        validate_backend((path,))

    def test_unverified_future_backend_is_not_assumed_to_support_policy(self):
        path = self.backend("future", "0.8.5")
        with self.assertRaisesRegex(RuntimeError, "session-policy patch"):
            validate_backend((path,))


if __name__ == "__main__":
    unittest.main()
