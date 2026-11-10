"""Ship Satellite notices and the complete modified MPL source with each release."""

import tarfile
from pathlib import Path

NOTICE = Path("share/doc/hearthspace/Satellite-LICENSE")
PROVENANCE = Path("share/doc/hearthspace/Satellite-PROVENANCE.md")
SOURCE = Path("share/doc/hearthspace/xwayland-satellite-source.tar.xz")
RELEASE_MODES = {str(path): 0o644 for path in (NOTICE, PROVENANCE, SOURCE)}


def source_archive(repository, release):
    source = repository / "vendor/xwayland-satellite"
    target = release / SOURCE
    target.parent.mkdir(parents=True, exist_ok=True)
    # Stable archive metadata avoids creating new release IDs for identical
    # source checkouts. Build caches and Git internals are never source inputs.
    with tarfile.open(target, "w:xz") as archive:
        for path in sorted(source.rglob("*")):
            relative = path.relative_to(source)
            if any(
                part in ("target", ".git", "__pycache__") for part in relative.parts
            ):
                continue
            if path.is_symlink():
                raise RuntimeError(f"Satellite source contains a symlink: {path}")
            if path.is_dir():
                continue
            if not path.is_file():
                raise RuntimeError(f"Satellite source is not a regular file: {path}")
            info = archive.gettarinfo(
                path, arcname=str(Path("xwayland-satellite") / relative)
            )
            info.uid = info.gid = info.mtime = 0
            info.uname = info.gname = ""
            info.mode = 0o755 if path.stat().st_mode & 0o111 else 0o644
            with path.open("rb") as stream:
                archive.addfile(info, stream)
    target.chmod(0o644)
    return target
