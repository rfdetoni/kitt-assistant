"""Bounded cold workspace snapshots; never checkpoint running processes or host mounts."""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tempfile
import time
import zipfile

from kitt.remote.cas_store import WorkspaceCasStore, cdc_chunk_stream, fixed_chunk_stream

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


def export_workspace_v2(
    workspace: Path,
    cas: WorkspaceCasStore,
    workspace_id: str,
    image: str,
    *,
    chunking: str = "fixed",
    parent_manifest: dict | None = None,
    metadata: dict | None = None,
) -> dict:
    """Incremental v2 chunked CAS snapshot preserving deleted paths and runtime metadata."""
    if chunking not in {"fixed", "cdc"}:
        raise ValueError("chunking must be 'fixed' or 'cdc'")
    entries: list[dict] = []
    seen_files: set[str] = set()
    total = count = reused_chunks = uploaded_chunks = 0
    stream_fn = cdc_chunk_stream if chunking == "cdc" else fixed_chunk_stream
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
                entries.append({"path": relative, "kind": "dir"})
                continue
            if not stat.S_ISREG(info.st_mode) or info.st_nlink != 1:
                raise ValueError("snapshot requires regular files")
            total += info.st_size
            if total > LIMIT:
                raise ValueError("snapshot size exceeds quota")
            seen_files.add(relative)
            chunks: list[dict] = []
            file_hash = hashlib.sha256()
            with path.open("rb") as source:
                for chunk in stream_fn(source):
                    file_hash.update(chunk)
                    cdigest, clen, created = cas.put_bytes(workspace_id, chunk)
                    uploaded_chunks += 1 if created else 0
                    reused_chunks += 0 if created else 1
                    chunks.append({"sha256": cdigest, "size": clen})
            entries.append({
                "path": relative,
                "kind": "file",
                "size": info.st_size,
                "sha256": file_hash.hexdigest(),
                "chunks": chunks,
            })
    deleted: list[str] = []
    if isinstance(parent_manifest, dict):
        parent_files = {
            e["path"] for e in parent_manifest.get("entries", [])
            if isinstance(e, dict) and e.get("kind") == "file" and isinstance(e.get("path"), str)
        }
        deleted = sorted(parent_files - seen_files)
    snapshot_id = f"snap-{int(time.time() * 1000)}-{hashlib.sha256(image.encode()).hexdigest()[:8]}"
    all_digests = [c["sha256"] for e in entries for c in e.get("chunks", [])]
    cas.bind_snapshot(workspace_id, snapshot_id, all_digests)
    return {
        "version": 2,
        "snapshot_id": snapshot_id,
        "workspace_id": workspace_id,
        "image": image,
        "chunking": chunking,
        "total_bytes": total,
        "entries": entries,
        "deleted": deleted,
        "metadata": dict(metadata or {}),
        "dedup": {"uploaded_chunks": uploaded_chunks, "reused_chunks": reused_chunks},
    }


def import_workspace_v2(
    workspace: Path,
    cas: WorkspaceCasStore,
    workspace_id: str,
    manifest: dict,
    image: str,
) -> dict:
    """Restore a v2 CAS snapshot manifest into workspace and prune deleted paths."""
    if not isinstance(manifest, dict) or manifest.get("version") != 2 or manifest.get("image") != image:
        raise ValueError("snapshot image or version mismatch")
    if workspace.is_symlink():
        raise ValueError("unsafe workspace symlink")
    workspace.mkdir(parents=True, exist_ok=True)
    for rel_str in manifest.get("deleted", []):
        rel = _safe_name(rel_str)
        target = workspace.joinpath(*rel.parts)
        if target.exists() and not target.is_symlink() and target.is_file():
            target.unlink()
    restored_files = 0
    restored_bytes = 0
    for entry in manifest.get("entries", []):
        rel = _safe_name(entry["path"])
        target = workspace.joinpath(*rel.parts)
        if entry["kind"] == "dir":
            target.mkdir(parents=True, exist_ok=True)
            continue
        target.parent.mkdir(parents=True, exist_ok=True)
        h = hashlib.sha256()
        with target.open("wb") as out:
            for chunk_meta in entry.get("chunks", []):
                data = cas.get_bytes(workspace_id, chunk_meta["sha256"])
                h.update(data)
                out.write(data)
                restored_bytes += len(data)
        if h.hexdigest() != entry["sha256"]:
            raise ValueError(f"restored file digest mismatch: {entry['path']}")
        restored_files += 1
    return {"files": restored_files, "bytes": restored_bytes, "snapshot_id": manifest.get("snapshot_id")}
