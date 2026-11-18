"""Authenticated command-line adapter for the opt-in Cortex event inbox."""
from __future__ import annotations

import argparse
import asyncio
import json
from pathlib import Path
import sys

from kitt.daemon.client import DaemonClient


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="kitt-cortex", description="KITT event-plane management")
    p.add_argument("--workspace", default=".", help="Workspace served by the existing daemon")
    commands = p.add_subparsers(dest="command", required=True)
    commands.add_parser("status")
    publish = commands.add_parser("publish", help="Publish external, untrusted event")
    publish.add_argument("topic")
    publish.add_argument("--source", default="cli")
    publish.add_argument("--payload", required=True, help="JSON object; max 16 KiB")
    publish.add_argument("--id", dest="event_id")
    publish.add_argument("--severity", type=int, default=1)
    publish.add_argument("--sensitivity", default="private")
    claim = commands.add_parser("claim")
    claim.add_argument("consumer")
    claim.add_argument("--limit", type=int, default=10)
    settle = commands.add_parser("settle")
    settle.add_argument("consumer")
    settle.add_argument("event_id")
    settle.add_argument("lease_token")
    settle.add_argument("--failed", action="store_true")
    update = commands.add_parser("context-set")
    update.add_argument("field")
    update.add_argument("value")
    commands.add_parser("context-get")
    preflight = commands.add_parser("preflight")
    preflight.add_argument("event_id")
    preflight.add_argument("effect")
    preflight.add_argument("tool_domain")
    route = commands.add_parser("route")
    route.add_argument("event_id")
    commands.add_parser("prune")
    return p


def make_request(args: argparse.Namespace) -> tuple[str, dict]:
    cmd = args.command
    if cmd == "status":
        return "cortex.status", {}
    if cmd == "publish":
        try:
            payload = json.loads(args.payload)
        except json.JSONDecodeError as exc:
            raise ValueError("payload must be valid JSON") from exc
        if not isinstance(payload, dict):
            raise ValueError("payload must be a JSON object")
        return "cortex.publish", {
            "topic": args.topic, "source": args.source, "payload": payload,
            "event_id": args.event_id, "severity": args.severity,
            "sensitivity": args.sensitivity,
        }
    if cmd == "claim":
        return "cortex.claim", {"consumer": args.consumer, "limit": args.limit}
    if cmd == "settle":
        return "cortex.settle", {
            "consumer": args.consumer, "event_id": args.event_id,
            "lease_token": args.lease_token, "success": not args.failed,
        }
    if cmd == "context-set":
        return "cortex.context.update", {"field": args.field, "value": args.value}
    if cmd == "context-get":
        return "cortex.context.get", {}
    if cmd == "preflight":
        return "cortex.preflight", {"event_id": args.event_id,
                                    "effect": args.effect, "tool_domain": args.tool_domain}
    if cmd == "route":
        return "cortex.route", {"event_id": args.event_id}
    if cmd == "prune":
        return "cortex.prune", {}
    raise ValueError("unknown command")


async def execute(args: argparse.Namespace) -> dict:
    action, params = make_request(args)
    client = DaemonClient(workspace_root=Path(args.workspace).resolve())
    if not await client.connect():
        raise ConnectionError("No compatible authenticated KITT daemon is available")
    try:
        response = await client.send_request(action, params, timeout=12.0)
    finally:
        await client.close()
    if response.get("status") != "ok":
        raise RuntimeError(str(response.get("error") or "Cortex request failed"))
    return {k: v for k, v in response.items()
            if k not in {"type", "request_id", "status", "action"}}


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    try:
        print(json.dumps(asyncio.run(execute(args)), ensure_ascii=False, indent=2))
    except (OSError, ConnectionError, RuntimeError, ValueError) as exc:
        print(f"kitt-cortex: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
