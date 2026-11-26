"""Workspace-isolated content-addressed blob store, CDC chunking, and adaptive Zstd."""
from __future__ import annotations

import hashlib
import json
import os
import re
import threading
import time
from compression import zstd
from pathlib import Path
from typing import Any, BinaryIO, Iterator

try:
    import fcntl
except ImportError:  # pragma: no cover
    fcntl = None  # type: ignore[assignment]

ZSTD_MAGIC = b"\x28\xb5\x2f\xfd"
INCOMPRESSIBLE_PREFIXES = (
    ZSTD_MAGIC,
    b"\x1f\x8b",        # gzip
    b"PK\x03\x04",      # zip
    b"\x89PNG\r\n\x1a\n",
    b"\xff\xd8\xff",    # jpeg
    b"BZh",
    b"\xfd7zXZ\x00",
    b"GGUF",
)
ADAPTIVE_MIN_BYTES = 4096
ZSTD_WINDOW_LOG_MAX = 20  # 1 MiB bounded decoder window
_SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
_SAFE_ID_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9_.:-]{0,127}$")


def should_compress_zstd(prefix: bytes, total_size: int | None = None) -> bool:
    if total_size is not None and total_size < ADAPTIVE_MIN_BYTES:
        return False
    return not any(prefix.startswith(sig) for sig in INCOMPRESSIBLE_PREFIXES)


def compress_zstd_bytes(data: bytes, *, level: int = 3) -> bytes:
    comp = zstd.ZstdCompressor(level=level)
    return comp.compress(data, zstd.ZstdCompressor.FLUSH_FRAME)


def decompress_zstd_bytes(data: bytes, *, max_bytes: int, window_log_max: int = ZSTD_WINDOW_LOG_MAX) -> bytes:
    """Single-frame bounded Zstd decompression rejecting zip-bombs and trailing frames."""
    dec = zstd.ZstdDecompressor(options={zstd.DecompressionParameter.window_log_max: window_log_max})
    out = dec.decompress(data, max_length=max_bytes + 1)
    if len(out) > max_bytes:
        raise ValueError(f"zstd payload exceeds uncompressed limit {max_bytes}")
    if dec.needs_input or not dec.eof:
        raise ValueError("incomplete zstd frame")
    if dec.unused_data:
        raise ValueError("trailing data after zstd frame")
    return out


_GEAR = tuple(((i * 0x9E3779B185EBCA87 + 0xC2B2AE3D27D4EB4F) >> 16) & 0xFFFFFFFF for i in range(256))


def cdc_chunk_stream(
    stream: BinaryIO,
    *,
    min_size: int = 64 * 1024,
    avg_size: int = 256 * 1024,
    max_size: int = 1024 * 1024,
) -> Iterator[bytes]:
    """Gear-hash content-defined chunking with bounded min/max cut points."""
    if min_size <= 0 or avg_size < min_size or max_size < avg_size:
        raise ValueError("invalid CDC size bounds")
    mask = max(1, (1 << (avg_size.bit_length() - 1)) - 1)
    buf = bytearray()
    h = 0
    read_size = 64 * 1024
    while True:
        chunk = stream.read(read_size)
        if not chunk:
            break
        buf.extend(chunk)
        pos = 0
        limit = len(buf)
        while pos < limit:
            remaining = limit - pos
            if remaining < min_size:
                break
            end_scan = min(limit, pos + max_size)
            cut = None
            i = pos + min_size
            while i < end_scan:
                h = ((h << 1) + _GEAR[buf[i]]) & 0xFFFFFFFF
                if (h & mask) == 0:
                    cut = i + 1
                    break
                i += 1
            if cut is None and remaining >= max_size:
                cut = pos + max_size
            if cut is None:
                break
            yield bytes(buf[pos:cut])
            pos = cut
            h = 0
        if pos:
            del buf[:pos]
    if buf:
        yield bytes(buf)


def fixed_chunk_stream(stream: BinaryIO, *, chunk_size: int = 256 * 1024) -> Iterator[bytes]:
    if chunk_size <= 0:
        raise ValueError("chunk_size must be positive")
    while True:
        part = stream.read(chunk_size)
        if not part:
            break
        yield part


class WorkspaceCasStore:
    """Content-addressed blob storage isolated per workspace with reference-counted GC."""

    def __init__(self, root: Path | str, *, max_bytes: int = 512 * 1024 * 1024) -> None:
        self.root = Path(root)
        self.max_bytes = max_bytes
        self._lock = threading.RLock()
        self.root.mkdir(parents=True, exist_ok=True)
        (self.root / "blobs").mkdir(parents=True, exist_ok=True)
        self._refs_path = self.root / "refs.json"
        self._flock_path = self.root / "refs.lock"

    @staticmethod
    def _validate_digest(digest: str) -> str:
        d = str(digest or "").strip().lower()
        if not _SHA256_RE.fullmatch(d):
            raise ValueError(f"invalid sha256 digest: {digest}")
        return d

    @staticmethod
    def _validate_id(value: str, label: str) -> str:
        text = str(value or "").strip()
        if not _SAFE_ID_RE.fullmatch(text):
            raise ValueError(f"invalid {label}: {value}")
        return text

    def blob_path(self, workspace_id: str, digest: str) -> Path:
        ws = self._validate_id(workspace_id, "workspace_id")
        d = self._validate_digest(digest)
        path = (self.root / "blobs" / ws / d[:2] / d).resolve()
        base = (self.root / "blobs" / ws).resolve()
        if not path.is_relative_to(base):
            raise ValueError("blob path escaped workspace CAS")
        return path

    def _with_refs(self, fn: Any) -> Any:
        fd = os.open(self._flock_path, os.O_RDWR | os.O_CREAT, 0o600)
        with self._lock, os.fdopen(fd, "a+", encoding="utf-8") as lf:
            if fcntl is not None:
                fcntl.flock(lf.fileno(), fcntl.LOCK_EX)
            try:
                data = {"version": 1, "workspaces": {}}
                if self._refs_path.exists():
                    try:
                        loaded = json.loads(self._refs_path.read_text(encoding="utf-8"))
                        if isinstance(loaded, dict) and isinstance(loaded.get("workspaces"), dict):
                            data = loaded
                    except (OSError, json.JSONDecodeError):
                        pass
                res, dirty = fn(data)
                if dirty:
                    tmp = self._refs_path.with_name(f".refs.{os.getpid()}.tmp")
                    wfd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
                    with os.fdopen(wfd, "w", encoding="utf-8") as handle:
                        json.dump(data, handle, sort_keys=True, separators=(",", ":"))
                        handle.flush()
                        os.fsync(handle.fileno())
                    os.replace(tmp, self._refs_path)
                return res
            finally:
                if fcntl is not None:
                    fcntl.flock(lf.fileno(), fcntl.LOCK_UN)

    def put_bytes(self, workspace_id: str, data: bytes, *, snapshot_id: str | None = None) -> tuple[str, int, bool]:
        digest = hashlib.sha256(data).hexdigest()
        dest = self.blob_path(workspace_id, digest)
        dest.parent.mkdir(parents=True, exist_ok=True)
        existed = dest.exists()
        if not existed:
            tmp = dest.with_name(f".{digest}.{os.getpid()}.tmp")
            fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            try:
                with os.fdopen(fd, "wb") as handle:
                    handle.write(data)
                    handle.flush()
                    os.fsync(handle.fileno())
                os.replace(tmp, dest)
            finally:
                if tmp.exists():
                    tmp.unlink(missing_ok=True)
        if snapshot_id is not None:
            self.bind_snapshot(workspace_id, snapshot_id, [digest])
        return digest, len(data), not existed

    def has_blob(self, workspace_id: str, digest: str) -> bool:
        return self.blob_path(workspace_id, digest).is_file()

    def get_bytes(self, workspace_id: str, digest: str, *, max_bytes: int | None = None) -> bytes:
        path = self.blob_path(workspace_id, digest)
        if not path.is_file():
            raise FileNotFoundError(digest)
        size = path.stat().st_size
        limit = self.max_bytes if max_bytes is None else min(self.max_bytes, max_bytes)
        if size > limit:
            raise ValueError(f"blob {digest} exceeds limit {limit}")
        data = path.read_bytes()
        if hashlib.sha256(data).hexdigest() != self._validate_digest(digest):
            raise ValueError(f"corrupt CAS blob {digest}")
        return data

    def bind_snapshot(self, workspace_id: str, snapshot_id: str, digests: list[str]) -> None:
        ws = self._validate_id(workspace_id, "workspace_id")
        snap = self._validate_id(snapshot_id, "snapshot_id")
        clean_digests = sorted({self._validate_digest(d) for d in digests})

        def update(data: dict[str, Any]) -> tuple[None, bool]:
            ws_map = data["workspaces"].setdefault(ws, {})
            ws_map[snap] = clean_digests
            return None, True

        self._with_refs(update)

    def unbind_snapshot(self, workspace_id: str, snapshot_id: str) -> None:
        ws = self._validate_id(workspace_id, "workspace_id")
        snap = self._validate_id(snapshot_id, "snapshot_id")

        def update(data: dict[str, Any]) -> tuple[None, bool]:
            ws_map = data["workspaces"].get(ws)
            if isinstance(ws_map, dict) and snap in ws_map:
                ws_map.pop(snap, None)
                return None, True
            return None, False

        self._with_refs(update)

    def gc(self, workspace_id: str, *, min_tmp_age_seconds: float = 300.0) -> dict[str, int]:
        """Mark-and-sweep referenced blobs only, plus clean stale temp files."""
        ws = self._validate_id(workspace_id, "workspace_id")
        referenced: set[str] = self._with_refs(
            lambda data: (
                {d for digests in data["workspaces"].get(ws, {}).values() for d in digests},
                False,
            )
        )
        base = self.root / "blobs" / ws
        removed_blobs = 0
        removed_bytes = 0
        removed_tmp = 0
        now = time.time()
        if base.exists():
            for path in base.rglob("*"):
                if not path.is_file():
                    continue
                if path.name.endswith(".tmp"):
                    try:
                        if now - path.stat().st_mtime >= min_tmp_age_seconds:
                            path.unlink(missing_ok=True)
                            removed_tmp += 1
                    except OSError:
                        pass
                    continue
                if _SHA256_RE.fullmatch(path.name) and path.name not in referenced:
                    try:
                        sz = path.stat().st_size
                        path.unlink(missing_ok=True)
                        removed_blobs += 1
                        removed_bytes += sz
                    except OSError:
                        pass
        return {
            "removed_blobs": removed_blobs,
            "removed_bytes": removed_bytes,
            "removed_tmp": removed_tmp,
            "kept_refs": len(referenced),
        }
