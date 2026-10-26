"""Own only the fixed locker policy; preserve administrator-managed PAM files."""

import hashlib
import os
import stat
import tempfile
from pathlib import Path

from .filesystem import atomic_write, directories, no_symlinks
from .layout import digest

POLICY = Path("etc/pam.d/hearthspace-lock")
OWNER_HEADER = b"# Managed by Hearthspace installation: "


class Policy:
    def __init__(self, system_root, prefix, record):
        if not system_root.is_absolute() or ".." in system_root.parts:
            raise RuntimeError("PAM system root must be absolute without parents")
        self.path = system_root / POLICY
        self.uid = 0 if system_root == Path("/") else os.geteuid()
        self.gid = 0 if system_root == Path("/") else os.getegid()
        self.header = OWNER_HEADER + str(prefix).encode() + b"\n"
        self.record = record
        self.before = None
        self.changed = False

    def verify_permissions(self):
        no_symlinks(self.path)
        info = self.path.stat()
        if (
            not stat.S_ISREG(info.st_mode)
            or info.st_uid != self.uid
            or info.st_gid != self.gid
            or stat.S_IMODE(info.st_mode) != 0o644
        ):
            raise RuntimeError(
                f"PAM policy must be a regular file owned by {self.uid}:{self.gid} "
                f"with mode 0644: {self.path}"
            )

    def verify_owned(self):
        if self.record is None:
            return
        if self.record.get("path") != str(POLICY) or self.record.get("mode") != 0o644:
            raise RuntimeError("unknown managed PAM policy in installation manifest")
        no_symlinks(self.path)
        if not self.path.is_file():
            raise RuntimeError(f"owned PAM policy changed or missing: {self.path}")
        self.verify_permissions()
        if digest(self.path) != self.record["sha256"]:
            raise RuntimeError(f"owned PAM policy changed or missing: {self.path}")

    def install(self, content):
        no_symlinks(self.path)
        content = self.header + content
        if self.record is not None:
            self.verify_owned()
            self.before = self.path.read_bytes()
            if self.before == content:
                return
            atomic_write(self.path, content, 0o644, owner=(self.uid, self.gid))
            self.changed = True
        elif self.path.exists():
            self.verify_permissions()
            if self.path.read_bytes().startswith(OWNER_HEADER):
                raise RuntimeError(
                    f"PAM policy belongs to another installation: {self.path}"
                )
            # Even an identical administrator policy stays unowned.
            return
        else:
            directories(self.path.parent)
            descriptor, temporary = tempfile.mkstemp(
                prefix=".hearthspace-pam-", dir=self.path.parent
            )
            try:
                with os.fdopen(descriptor, "wb") as stream:
                    stream.write(content)
                    os.fchown(stream.fileno(), self.uid, self.gid)
                    os.fchmod(stream.fileno(), 0o644)
                    stream.flush()
                    os.fsync(stream.fileno())
                # Publish complete bytes without replacing a concurrently created policy.
                os.link(temporary, self.path)
                self.changed = True
            finally:
                Path(temporary).unlink(missing_ok=True)
        self.record = {
            "path": str(POLICY),
            "mode": 0o644,
            "sha256": hashlib.sha256(content).hexdigest(),
        }
        self.verify_permissions()

    def rollback(self):
        if not self.changed:
            return
        self.verify_owned()
        if self.before is None:
            self.path.unlink()
        else:
            atomic_write(self.path, self.before, 0o644, owner=(self.uid, self.gid))

    def remove(self):
        if self.record is not None:
            self.verify_owned()
            self.path.unlink()
