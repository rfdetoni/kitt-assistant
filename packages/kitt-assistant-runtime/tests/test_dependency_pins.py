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



def test_agent_cli_lock_checkout_and_python_dependency_agree():
    root = Path(__file__).resolve().parents[3]
    package_dir = root / "packages" / "kitt-assistant-runtime"
    lock = tomllib.loads((package_dir / "uv.lock").read_text(encoding="utf-8"))
    agent = next(item for item in lock["package"] if item["name"] == "kitt-agent-cli")
    match = re.fullmatch(
        r"https://github\.com/rfdetoni/kitt-agent-cli\.git\?branch=main#([0-9a-f]{40})",
        agent["source"]["git"],
    )
    assert match, "Agent CLI lock must use a pinned main SHA"
    sha = match.group(1)

    project = tomllib.loads((package_dir / "pyproject.toml").read_text(encoding="utf-8"))
    assert f"kitt-agent-cli>={agent['version']},<0.90" in project["project"]["dependencies"]
    assistant = next(item for item in lock["package"] if item["name"] == "kitt-assistant-runtime")
    assert assistant["version"] == project["project"]["version"]

    ci = (root / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
    assert ci.count(f"ref: {sha}") == 2, "Both CI agent checkouts must use the lock revision"
