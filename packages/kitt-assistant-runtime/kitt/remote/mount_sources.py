"""Operator-only immutable GitHub/S3 mount materialization; no guest credentials."""
from __future__ import annotations

from datetime import UTC, datetime
import gzip
import hashlib
import hmac
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import tarfile
import tempfile
import time
import urllib.request
from urllib.parse import quote, urlsplit
import xml.etree.ElementTree as ET

_LIMIT = 128 * 1024 * 1024
_ARCHIVE_LIMIT = 32 * 1024 * 1024
_HASH = re.compile(r"[0-9a-f]{64}")
_ALIAS = re.compile(r"[A-Za-z0-9][A-Za-z0-9_-]{0,63}")
_REPO = re.compile(r"[A-Za-z0-9_.-]{1,100}/[A-Za-z0-9_.-]{1,100}")
_EMPTY_SHA256 = hashlib.sha256(b"").hexdigest()


def _relative(value):
    if not isinstance(value, str) or not value or len(value) > 1024 or "\\" in value or ":" in value:
        raise ValueError("invalid mount member path")
    parts = value.split("/")
    if any(part in {"", ".", ".."} or any(ord(c) < 32 for c in part) for part in parts):
        raise ValueError("unsafe mount member path")
    return PurePosixPath(*parts)


def _digest_pin(value):
    if not isinstance(value, str) or not _HASH.fullmatch(value):
        raise ValueError("mount requires a pinned SHA256")
    return value


class _GithubRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urlsplit(newurl)
        if target.scheme != "https" or target.netloc != "codeload.github.com" or target.username:
            raise ValueError("untrusted GitHub archive redirect")
        # The GitHub API token must not be forwarded to codeload.
        return urllib.request.Request(newurl, headers={"User-Agent": "KITT-worker-mount"})


def _github_archive(spec, archive):
    repository, revision = spec.get("repository"), spec.get("revision")
    if (not isinstance(repository, str) or not _REPO.fullmatch(repository)
            or any(part in {".", ".."} for part in repository.split("/"))
            or not isinstance(revision, str) or not re.fullmatch(r"[0-9a-f]{40}", revision)):
        raise ValueError("GitHub mount requires owner/repo and immutable commit")
    expected = _digest_pin(spec.get("sha256"))
    token = os.environ.get("KITT_GITHUB_MOUNT_TOKEN")
    headers = {"User-Agent": "KITT-worker-mount", "Accept": "application/vnd.github+json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(f"https://api.github.com/repos/{repository}/tarball/{revision}",
                                 headers=headers)
    opener = urllib.request.build_opener(_GithubRedirect())
    digest, total = hashlib.sha256(), 0
    deadline = time.monotonic() + 120
    with opener.open(req, timeout=20) as response, archive.open("xb") as target:
        if response.status != 200:
            raise RuntimeError("GitHub archive unavailable")
        while chunk := response.read(64 * 1024):
            total += len(chunk)
            if total > _ARCHIVE_LIMIT or time.monotonic() > deadline:
                raise ValueError("GitHub archive exceeds resource limit")
            digest.update(chunk)
            target.write(chunk)
    if digest.hexdigest() != expected:
        raise ValueError("GitHub archive digest mismatch")


class _BoundedReader:
    def __init__(self, source, limit):
        self.source, self.remaining = source, limit

    def read(self, size):
        data = self.source.read(min(size, self.remaining + 1))
        self.remaining -= len(data)
        if self.remaining < 0:
            raise ValueError("expanded archive exceeds limit")
        return data


def _extract_github(archive, target, budget):
    total = count = 0
    prefix = None
    with gzip.open(archive, "rb") as raw, tarfile.open(
        fileobj=_BoundedReader(raw, budget + 8 * 1024 * 1024), mode="r|",
    ) as stream:
        for member in stream:
            count += 1
            if count > 4096:
                raise ValueError("GitHub mount has too many entries")
            path = _relative(member.name.rstrip("/"))
            if prefix is None:
                prefix = path.parts[0]
            if path.parts[0] != prefix:
                raise ValueError("archive must have one root")
            if len(path.parts) == 1:
                if not member.isdir():
                    raise ValueError("invalid archive root")
                continue
            destination = target.joinpath(*path.parts[1:])
            if member.isdir():
                destination.mkdir(parents=True, exist_ok=True)
                continue
            if not member.isfile() or member.size < 0 or total + member.size > budget:
                raise ValueError("unsafe or oversized GitHub mount member")
            destination.parent.mkdir(parents=True, exist_ok=True)
            source = stream.extractfile(member)
            if source is None:
                raise ValueError("archive member missing data")
            with source, destination.open("xb") as output:
                shutil.copyfileobj(source, output, length=64 * 1024)
            if destination.stat().st_size != member.size:
                raise ValueError("truncated archive member")
            total += member.size
    if not count:
        raise ValueError("empty GitHub archive")
    return total


def _s3_sign_headers(
    method: str,
    endpoint: str,
    bucket: str,
    path_Suffix: str,
    query_params: dict[str, str],
    region: str,
    access_key: str,
    secret_key: str,
    session_token: str | None = None,
) -> tuple[str, dict[str, str]]:
    parsed = urlsplit(endpoint)
    if parsed.scheme not in {"https", "http"}:
        raise ValueError("S3 endpoint must use HTTPS or loopback HTTP")
    if parsed.scheme == "http" and parsed.hostname not in {"127.0.0.1", "localhost", "::1"}:
        raise ValueError("non-loopback HTTP S3 endpoint rejected")
    now = datetime.now(UTC)
    amz_date = now.strftime("%Y%m%dT%H%M%SZ")
    date_stamp = now.strftime("%Y%m%d")
    canonical_uri = f"/{quote(bucket, safe='-_.~')}{path_Suffix}"
    canonical_qs = "&".join(
        f"{quote(k, safe='-_.~')}={quote(v, safe='-_.~')}"
        for k, v in sorted(query_params.items())
    )
    host = parsed.netloc
    headers: dict[str, str] = {
        "host": host,
        "x-amz-content-sha256": _EMPTY_SHA256,
        "x-amz-date": amz_date,
    }
    if session_token:
        headers["x-amz-security-token"] = session_token
    signed_headers = ";".join(sorted(headers))
    canonical_headers = "".join(f"{k}:{headers[k]}\n" for k in sorted(headers))
    canonical_req = "\n".join([method, canonical_uri, canonical_qs, canonical_headers, signed_headers, _EMPTY_SHA256])
    scope = f"{date_stamp}/{region}/s3/aws4_request"
    string_to_sign = "\n".join(["AWS4-HMAC-SHA256", amz_date, scope, hashlib.sha256(canonical_req.encode()).hexdigest()])

    def _hmac(key: bytes, msg: str) -> bytes:
        return hmac.new(key, msg.encode("utf-8"), hashlib.sha256).digest()

    signing_key = _hmac(_hmac(_hmac(_hmac(("AWS4" + secret_key).encode("utf-8"), date_stamp), region), "s3"), "aws4_request")
    sig = hmac.new(signing_key, string_to_sign.encode("utf-8"), hashlib.sha256).hexdigest()
    auth = f"AWS4-HMAC-SHA256 Credential={access_key}/{scope}, SignedHeaders={signed_headers}, Signature={sig}"
    req_headers = {
        "Host": host,
        "x-amz-date": amz_date,
        "x-amz-content-sha256": _EMPTY_SHA256,
        "Authorization": auth,
    }
    if session_token:
        req_headers["x-amz-security-token"] = session_token
    url = f"{parsed.scheme}://{host}{canonical_uri}" + (f"?{canonical_qs}" if canonical_qs else "")
    return url, req_headers


def _s3_objects(spec, target, budget):
    bucket = spec.get("bucket")
    objects = spec.get("objects")
    if (not isinstance(bucket, str) or not re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]", bucket)
            or not isinstance(objects, list) or not 1 <= len(objects) <= 128):
        raise ValueError("S3 mount requires explicit bucket and 1..128 versioned objects")
    access_key = os.environ.get("AWS_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("AWS_SECRET_ACCESS_KEY", "").strip()
    session_token = os.environ.get("AWS_SESSION_TOKEN", "").strip() or None
    if not access_key or not secret_key:
        raise RuntimeError("AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY are required for S3 mounts")
    region = (os.environ.get("AWS_REGION") or os.environ.get("AWS_DEFAULT_REGION") or "us-east-1").strip()
    endpoint = (os.environ.get("KITT_S3_ENDPOINT_URL") or f"https://s3.{region}.amazonaws.com").rstrip("/")
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    total = 0
    for item in objects:
        if not isinstance(item, dict) or set(item) != {"key", "version_id", "sha256", "path"}:
            raise ValueError("invalid S3 object manifest")
        key, version = item["key"], item["version_id"]
        if (not isinstance(key, str) or not 1 <= len(key.encode("utf-8")) <= 1024
                or not isinstance(version, str) or not 1 <= len(version) <= 1024 or version == "null"
                or any(ord(c) < 32 for c in key + version)):
            raise ValueError("S3 object requires explicit version")
        expected = _digest_pin(item["sha256"])
        destination = target.joinpath(*_relative(item["path"]).parts)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists():
            raise ValueError("duplicate S3 mount path")
        quoted_key = "/" + "/".join(quote(seg, safe="-_.~") for seg in key.lstrip("/").split("/"))
        url, headers = _s3_sign_headers(
            "GET", endpoint, bucket, quoted_key, {"versionId": version},
            region, access_key, secret_key, session_token,
        )
        req = urllib.request.Request(url, headers=headers, method="GET")
        digest = hashlib.sha256()
        obj_size = 0
        with opener.open(req, timeout=30) as response, destination.open("xb") as out:
            if response.status != 200:
                raise RuntimeError("S3 object transfer failed")
            clen_hdr = response.headers.get("Content-Length")
            if clen_hdr is not None and clen_hdr.isdigit() and int(clen_hdr) > budget - total:
                raise ValueError("S3 mount exceeds size limit")
            while chunk := response.read(64 * 1024):
                obj_size += len(chunk)
                if obj_size > budget - total:
                    raise ValueError("S3 mount exceeds size limit")
                digest.update(chunk)
                out.write(chunk)
        if digest.hexdigest() != expected:
            raise ValueError("S3 object digest mismatch")
        total += obj_size
    return total


def list_s3_prefix(bucket: str, prefix: str = "") -> list[dict[str, str]]:
    """Bounded stdlib S3 ListObjectsV2 helper for operator inspection."""
    if not re.fullmatch(r"[a-z0-9][a-z0-9.-]{1,61}[a-z0-9]", bucket):
        raise ValueError("invalid S3 bucket")
    access_key = os.environ.get("AWS_ACCESS_KEY_ID", "").strip()
    secret_key = os.environ.get("AWS_SECRET_ACCESS_KEY", "").strip()
    if not access_key or not secret_key:
        raise RuntimeError("AWS credentials required")
    region = (os.environ.get("AWS_REGION") or os.environ.get("AWS_DEFAULT_REGION") or "us-east-1").strip()
    endpoint = (os.environ.get("KITT_S3_ENDPOINT_URL") or f"https://s3.{region}.amazonaws.com").rstrip("/")
    url, headers = _s3_sign_headers(
        "GET", endpoint, bucket, "/", {"list-type": "2", "max-keys": "128", "prefix": prefix},
        region, access_key, secret_key, os.environ.get("AWS_SESSION_TOKEN", "").strip() or None,
    )
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(urllib.request.Request(url, headers=headers, method="GET"), timeout=15) as resp:
        xml_bytes = resp.read(256 * 1024)
    root = ET.fromstring(xml_bytes)
    ns = {"s3": root.tag.split("}")[0].lstrip("{")} if "}" in root.tag else {}
    contents_tag = "s3:Contents" if ns else "Contents"
    key_tag = "s3:Key" if ns else "Key"
    size_tag = "s3:Size" if ns else "Size"
    items: list[dict[str, str]] = []
    for node in root.findall(contents_tag, ns):
        k = node.findtext(key_tag, default="", namespaces=ns)
        s = node.findtext(size_tag, default="0", namespaces=ns)
        if k:
            items.append({"key": k, "size": s})
    return items


def prepare_mounts(root: Path, manifest: Path):
    """Fetch once into private staging; mount only complete, immutable snapshots."""
    if manifest.is_symlink() or not manifest.is_file() or manifest.stat().st_size > 64 * 1024:
        raise ValueError("mount manifest must be a bounded regular file")
    raw = manifest.read_bytes()
    specs = json.loads(raw)
    if not isinstance(specs, list) or not 1 <= len(specs) <= 8:
        raise ValueError("mount manifest requires 1..8 source specifications")
    aliases = set()
    for spec in specs:
        if not isinstance(spec, dict):
            raise ValueError("invalid mount specification")
        alias, kind = spec.get("alias"), spec.get("kind")
        if (not isinstance(alias, str) or not _ALIAS.fullmatch(alias) or alias.lower() == "cache"
                or alias.lower() in aliases or kind not in {"github", "s3"}):
            raise ValueError("invalid or duplicate mount alias")
        allowed = {"alias", "kind", "repository", "revision", "sha256"} if kind == "github" else {
            "alias", "kind", "bucket", "objects",
        }
        if set(spec) != allowed:
            raise ValueError("unknown mount manifest field")
        aliases.add(alias.lower())
    parent = root / "mounts"
    parent.mkdir(mode=0o700, exist_ok=True)
    if parent.is_symlink() or not parent.is_dir():
        raise ValueError("unsafe mount storage")
    if os.name == "posix" and (parent.stat().st_uid != os.getuid() or parent.stat().st_mode & 0o077):
        raise ValueError("mount storage must be private")
    destination = parent / hashlib.sha256(raw).hexdigest()
    if destination.exists():
        if destination.is_symlink() or any(not (destination / spec["alias"]).is_dir()
                                            or (destination / spec["alias"]).is_symlink() for spec in specs):
            raise ValueError("invalid materialized mount")
    else:
        with tempfile.TemporaryDirectory(prefix=".stage-", dir=parent) as temporary:
            stage = Path(temporary)
            used = 0
            for spec in specs:
                target = stage / spec["alias"]
                target.mkdir()
                if spec["kind"] == "github":
                    archive = stage / ".archive"
                    _github_archive(spec, archive)
                    used += _extract_github(archive, target, _LIMIT - used)
                    archive.unlink()
                else:
                    used += _s3_objects(spec, target, _LIMIT - used)
            if destination.exists() or destination.is_symlink():
                raise RuntimeError("mount snapshot changed during materialization")
            stage.rename(destination)
    return {spec["alias"]: str(destination / spec["alias"]) for spec in specs}
