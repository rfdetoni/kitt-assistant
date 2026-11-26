from io import BytesIO
import hashlib
import json
import zipfile

import pytest

from kitt.remote.workspace_snapshot import export_workspace, import_workspace
from kitt.remote.mount_sources import _relative


def test_cold_workspace_roundtrip(tmp_path):
    src = tmp_path / "src"
    dst = tmp_path / "dst"
    src.mkdir()
    dst.mkdir()
    (src / "dir").mkdir()
    (src / "dir" / "result.bin").write_bytes(b"test" * 100)
    image = "example@sha256:" + "a" * 64
    stream = BytesIO()
    digest, size = export_workspace(src, stream, image)
    assert size == len(stream.getvalue())
    import_workspace(dst, stream, digest, image)
    assert (dst / "dir" / "result.bin").read_bytes() == b"test" * 100
    stream.seek(0)
    with pytest.raises(ValueError, match="empty"):
        import_workspace(dst, stream, digest, image)


def test_rejects_escape_and_wrong_image(tmp_path):
    dst = tmp_path / "dst"
    dst.mkdir()
    stream = BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        archive.writestr("manifest.json", json.dumps({"version": 1, "image": "image"}))
        archive.writestr("workspace/../outside", "bad")
    data = stream.getvalue()
    with pytest.raises(ValueError, match="image"):
        import_workspace(dst, BytesIO(data), hashlib.sha256(data).hexdigest(), "other")
    with pytest.raises(ValueError, match="unsafe"):
        import_workspace(dst, BytesIO(data), hashlib.sha256(data).hexdigest(), "image")
    assert not list(dst.iterdir())
    assert not (tmp_path / "outside").exists()


@pytest.mark.parametrize("name", ["../bad", "/etc", "C:/bad", "a/../b"])
def test_mount_rejects_unsafe_member(name):
    with pytest.raises(ValueError):
        _relative(name)
