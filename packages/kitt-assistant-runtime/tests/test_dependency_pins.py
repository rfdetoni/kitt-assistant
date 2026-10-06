"""Keep all Assistant Protocol consumers on the same immutable revision."""

import json
import re
import tomllib
from pathlib import Path


def test_protocol_lock_revision_and_version_agree_across_consumers():
    root = Path(__file__).resolve().parents[3]
    consumers = []
    for relative in ("Cargo.lock", "apps/kitt-hud/src-tauri/Cargo.lock"):
        packages = tomllib.loads((root / relative).read_text(encoding="utf-8"))["package"]
        protocol = next(package for package in packages if package["name"] == "kitt-protocol")
        consumers.append((relative, protocol["source"], protocol["version"]))

    relative = "packages/kitt-assistant-runtime/uv.lock"
    packages = tomllib.loads((root / relative).read_text(encoding="utf-8"))["package"]
    protocol = next(package for package in packages if package["name"] == "kitt-protocol")
    consumers.append((relative, protocol["source"]["git"], protocol["version"]))

    relative = "apps/kitt-hud/package-lock.json"
    packages = json.loads((root / relative).read_text(encoding="utf-8"))["packages"]
    protocol = packages["node_modules/@kitt/protocol"]
    consumers.append((relative, protocol["resolved"], protocol["version"]))

    revisions = {}
    versions = {}
    for relative, source, version in consumers:
        match = re.search(r"#([0-9a-f]{40})$", source)
        assert match, (relative, source)
        revisions[relative] = match.group(1)
        versions[relative] = version
    assert len(set(revisions.values())) == 1, revisions
    assert len(set(versions.values())) == 1, versions
