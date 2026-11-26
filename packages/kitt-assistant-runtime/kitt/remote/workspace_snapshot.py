"""Bounded cold workspace snapshots; never checkpoint running processes or host mounts."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tempfile
import zipfile

LIMIT = 8 * 1024 * 1024
MAX_ENTRIES = 4096


def _safe_name(name):
    parts = name.split("/")
    if (not name or len(name) > 1024 or "\\" in name or ":" in name
            or any(ord(c) < 32 for c in name)
            or any(part in {"", ".", ".."} for part in parts)):
        raise ValueError("unsafe snapshot member")
    return PurePosixPath(*parts)


def export_workspace(workspace, target, image):
    total = 0
    count = 0
    with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("manifest.json", json.dumps({"version": 1, "image": image}))
        for root, dirs, files in os.walk(workspace, followlinks=False):
            for name in sorted(dirs + files):
                path = Path(root) / name
                relative = str(path.relative_to(workspace).as_posix())
                _safe_name(relative)
                info = path.lstat()
                count += 1
                if count > MAX_ENTRIES or path.is_symlink():
                    raise ValueError("unsupported snapshot entry or quota exceeded")
                if stat.S_ISDIR(info.st_mode):
                    archive.writestr("workspace/" + relative + "/", b"")
                    continue
                if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                    raise ValueError("snapshot requires regular files")
                total += info.st_size
                if total > LIMIT:
                    raise ValueError("snapshot size exceeds quota")
                with path.open("rb") as source, archive.open("workspace/" + relative, "w") as out:
                    remaining = info.st_size
                    while remaining:
                        chunk = source.read(min(remaining, 65536))
                        if not chunk:
                            raise ValueError("workspace changed during snapshot")
                        out.write(chunk)
                        remaining -= len(chunk)
                    if source.read(1):
                        raise ValueError("workspace changed during snapshot")
    target.seek(0, os.SEEK_END)
    size = target.tell()
    if size > LIMIT:
        raise ValueError("snapshot archive exceeds quota")
    target.seek(0)
    digest = hashlib.file_digest(target, "sha256").hexdigest()
    target.seek(0)
    return digest, size


def import_workspace(workspace, source, digest, image):
    if workspace.is_symlink() or any(workspace.iterdir()):
        raise ValueError("snapshot destination must be an empty workspace")
    if hashlib.file_digest(source, "sha256").hexdigest() != digest:
        raise ValueError("snapshot digest mismatch")
    source.seek(0)
    with tempfile.TemporaryDirectory(prefix=".restore-", dir=workspace.parent) as folder:
        stage = Path(folder) / "workspace"
        stage.mkdir(mode=0o700)
        with zipfile.ZipFile(source) as archive:
            entries = archive.infolist()
            if len(entries) > MAX_ENTRIES + 1 or sum(i.file_size for i in entries) > LIMIT:
                raise ValueError("snapshot expansion quota exceeded")
            if len({i.filename for i in entries}) != len(entries):
                raise ValueError("duplicate snapshot member")
            manifest = archive.getinfo("manifest.json")
            if manifest.file_size > 4096 or json.loads(archive.read(manifest)) != {"version": 1, "image": image}:
                raise ValueError("snapshot image does not match destination")
            for entry in entries:
                if entry.filename == "manifest.json":
                    continue
                if not entry.filename.startswith("workspace/") or entry.compress_type != zipfile.ZIP_STORED:
                    raise ValueError("unsupported snapshot member")
                relative = _safe_name(entry.filename.removeprefix("workspace/").removesuffix("/"))
                mode = entry.external_attr >> 16
                if stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR):
                    raise ValueError("unsupported snapshot file type")
                path = stage.joinpath(*relative.parts)
                if entry.is_dir():
                    path.mkdir(parents=True, exist_ok=True)
                    continue
                path.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(entry) as inp, path.open("xb") as out:
                    shutil.copyfileobj(inp, out, 65536)
        if workspace.is_symlink() or any(workspace.iterdir()):
            raise ValueError("snapshot destination changed")
        os.replace(stage, workspace)
