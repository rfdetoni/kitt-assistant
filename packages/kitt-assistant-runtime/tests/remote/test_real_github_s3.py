"""Tests for pure-stdlib S3 SigV4 mount materialization and real public GitHub mounts."""
from __future__ import annotations

import hashlib
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import threading
import urllib.error

import pytest

from kitt.remote.mount_sources import list_s3_prefix, prepare_mounts


def test_stdlib_s3_sigv4_mount_and_list(tmp_path, monkeypatch):
    payload = b"model-weights-v1"
    digest = hashlib.sha256(payload).hexdigest()
    seen_auth: list[str] = []

    class FakeS3Handler(BaseHTTPRequestHandler):
        def log_message(self, fmt, *args):
            pass

        def do_GET(self):
            seen_auth.append(self.headers.get("Authorization", ""))
            if "list-type=2" in self.path:
                xml = (
                    b'<?xml version="1.0" encoding="UTF-8"?>'
                    b'<ListBucketResult><Contents><Key>weights.bin</Key><Size>16</Size></Contents></ListBucketResult>'
                )
                self.send_response(200)
                self.send_header("Content-Length", str(len(xml)))
                self.end_headers()
                self.wfile.write(xml)
                return
            if "versionId=v1" in self.path:
                self.send_response(200)
                self.send_header("Content-Length", str(len(payload)))
                self.end_headers()
                self.wfile.write(payload)
                return
            self.send_response(404)
            self.end_headers()

    srv = HTTPServer(("127.0.0.1", 0), FakeS3Handler)
    t = threading.Thread(target=srv.serve_forever, daemon=True)
    t.start()
    try:
        monkeypatch.setenv("AWS_ACCESS_KEY_ID", "AKIATESTKEY12345678")
        monkeypatch.setenv("AWS_SECRET_ACCESS_KEY", "SecretTestKey1234567890abcdefghijklmnop")
        monkeypatch.setenv("AWS_REGION", "us-east-1")
        monkeypatch.setenv("KITT_S3_ENDPOINT_URL", f"http://127.0.0.1:{srv.server_port}")

        listed = list_s3_prefix("test-bucket", "weights")
        assert listed == [{"key": "weights.bin", "size": "16"}]

        manifest = tmp_path / "mounts.json"
        manifest.write_text(json.dumps([{
            "alias": "weights",
            "kind": "s3",
            "bucket": "test-bucket",
            "objects": [{
                "key": "weights.bin",
                "version_id": "v1",
                "sha256": digest,
                "path": "weights.bin",
            }],
        }]))
        root = tmp_path / "state"
        root.mkdir(mode=0o700)
        mounted = prepare_mounts(root, manifest)
        assert (root / "mounts" / hashlib.sha256(manifest.read_bytes()).hexdigest() / "weights" / "weights.bin").read_bytes() == payload
        assert any("AWS4-HMAC-SHA256" in h and "us-east-1/s3/aws4_request" in h for h in seen_auth)
        assert mounted["weights"].endswith("/weights")
    finally:
        srv.shutdown()
        srv.server_close()
        t.join(timeout=3)


def test_real_public_github_immutable_mount_if_online(tmp_path):
    """Fetches real pinned rfdetoni/kitt-protocol commit if GitHub reachable, checks digest mismatch fail-closed."""
    manifest = tmp_path / "gh.json"
    bad_hash = "0" * 64
    manifest.write_text(json.dumps([{
        "alias": "proto",
        "kind": "github",
        "repository": "rfdetoni/kitt-protocol",
        "revision": "2cb05bb03e54eb2dc5f81bf151edcbd207ff194e",
        "sha256": bad_hash,
    }]))
    root = tmp_path / "gh-state"
    root.mkdir(mode=0o700)
    try:
        with pytest.raises(ValueError, match="digest mismatch"):
            prepare_mounts(root, manifest)
    except urllib.error.URLError:
        pytest.skip("network unavailable for live GitHub tarball verification")
