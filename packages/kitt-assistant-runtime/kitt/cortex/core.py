"""Durable, workspace-scoped event inbox and minimal digital-twin projection.

This is not an alternative Agent EventLedger or kitt-memoryd. Deliveries are
at-least-once; consumers must use existing Agent execution receipts for effects.
"""
from __future__ import annotations

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import time
import uuid

TOPICS = {
    "plugin-home-sec": ("home.",),
    "plugin-saas-sre": ("saas.",),
    "plugin-chief-of-staff": ("communications.", "calendar.", "user."),
    "plugin-staff-dev": ("dev.", "repo."),
}
NAME = re.compile(r"^[a-z][a-z0-9_.-]{0,79}$")
EVENT_ID = re.compile(r"^[A-Za-z0-9_.:-]{1,128}$")
TWIN_FIELDS = frozenset({"location", "activity_state", "active_channel"})
SENSITIVITIES = frozenset({"public", "personal", "private", "secret", "ephemeral"})
MAX_BODY_BYTES = 16 * 1024
MAX_BACKLOG = 10_000
MAX_ATTEMPTS = 5


def _name(value: str, what: str) -> str:
    if not isinstance(value, str) or NAME.fullmatch(value) is None:
        raise ValueError(f"invalid {what}")
    return value


def _json(value: object) -> str:
    try:
        result = json.dumps(value, separators=(",", ":"), ensure_ascii=False, allow_nan=False)
    except (TypeError, ValueError) as exc:
        raise ValueError("payload must be JSON-serializable") from exc
    if len(result.encode("utf-8")) > MAX_BODY_BYTES:
        raise ValueError("payload exceeds 16 KiB")
    return result


class CortexStore:
    """SQLite WAL inbox; no long-lived connections and no model calls on the hot path."""

    def __init__(self, workspace: str | Path, data_dir: str | Path | None = None):
        self.workspace = str(Path(workspace).resolve())
        root = Path(data_dir) if data_dir is not None else Path.home() / ".kitt" / "cortex"
        if root.is_symlink():
            raise PermissionError("Cortex data directory cannot be a symlink")
        root.mkdir(mode=0o700, parents=True, exist_ok=True)
        if root.is_symlink() or not root.is_dir():
            raise PermissionError("Cortex data directory must be a real directory")
        if os.name == "posix":
            if root.stat().st_uid != os.getuid():
                raise PermissionError("Cortex data directory owner mismatch")
            os.chmod(root, 0o700)
        suffix = hashlib.sha256(self.workspace.encode()).hexdigest()[:24]
        self.path = root / f"{suffix}.sqlite3"
        if self.path.is_symlink():
            raise PermissionError("Cortex database cannot be a symlink")
        # SQLite WAL permits concurrent readers while writers use short IMMEDIATE transactions.
        with self._db() as conn:
            conn.execute("PRAGMA journal_mode=WAL")
        with self._db(write=True) as conn:
            conn.executescript("""
                CREATE TABLE IF NOT EXISTS events (
                    id TEXT PRIMARY KEY, workspace TEXT NOT NULL,
                    topic TEXT NOT NULL, source TEXT NOT NULL,
                    severity INTEGER NOT NULL, untrusted INTEGER NOT NULL,
                    sensitivity TEXT NOT NULL, payload_json TEXT NOT NULL,
                    digest TEXT NOT NULL, created REAL NOT NULL,
                    expires REAL NOT NULL
                );
                CREATE TABLE IF NOT EXISTS deliveries (
                    event_id TEXT NOT NULL REFERENCES events(id) ON DELETE CASCADE,
                    consumer TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'READY',
                    attempts INTEGER NOT NULL DEFAULT 0, next_at REAL NOT NULL DEFAULT 0,
                    lease_token TEXT, lease_until REAL,
                    PRIMARY KEY(event_id,consumer)
                );
                CREATE INDEX IF NOT EXISTS deliveries_pick
                    ON deliveries(consumer,status,next_at,lease_until);
                CREATE INDEX IF NOT EXISTS events_retention ON events(expires);
                CREATE TABLE IF NOT EXISTS context (
                    field TEXT PRIMARY KEY, value_json TEXT NOT NULL,
                    source TEXT NOT NULL, sensitivity TEXT NOT NULL,
                    observed REAL NOT NULL, expires REAL NOT NULL, revision INTEGER NOT NULL
                );
            """)
        if os.name == "posix":
            os.chmod(self.path, 0o600)

    @contextmanager
    def _db(self, *, write: bool = False):
        conn = sqlite3.connect(self.path, timeout=5.0, isolation_level=None)
        conn.row_factory = sqlite3.Row
        try:
            conn.execute("PRAGMA busy_timeout=5000")
            conn.execute("PRAGMA foreign_keys=ON")
            if write:
                conn.execute("BEGIN IMMEDIATE")
            yield conn
            if write:
                conn.commit()
        except BaseException:
            if write:
                conn.rollback()
            raise
        finally:
            conn.close()

    def publish(self, *, topic: str, source: str, payload: dict,
                severity: int = 1, sensitivity: str = "private",
                event_id: str | None = None, ttl_seconds: int = 3600,
                untrusted: bool = True) -> dict:
        topic, source = _name(topic, "topic"), _name(source, "source")
        if not isinstance(payload, dict):
            raise ValueError("event payload must be an object")
        if type(severity) is not int or severity not in range(1, 6):
            raise ValueError("severity must be an integer from 1 to 5")
        if sensitivity not in SENSITIVITIES:
            raise ValueError("invalid sensitivity")
        if type(ttl_seconds) is not int or not 30 <= ttl_seconds <= 86400:
            raise ValueError("TTL must be 30..86400 seconds")
        ident = event_id or uuid.uuid4().hex
        if not isinstance(ident, str) or EVENT_ID.fullmatch(ident) is None:
            raise ValueError("invalid event id")
        body = _json(payload)
        # Identity includes trust so a repeated ID cannot be used to elevate authority.
        digest = hashlib.sha256(_json([topic, source, severity, bool(untrusted),
                                        sensitivity, body]).encode()).hexdigest()
        now = time.time()
        consumers = [name for name, prefixes in TOPICS.items()
                     if any(topic.startswith(p) for p in prefixes)]
        with self._db(write=True) as db:
            db.execute("DELETE FROM events WHERE expires<=?", (now,))
            db.execute("DELETE FROM context WHERE expires<=?", (now,))
            previous = db.execute("SELECT digest FROM events WHERE id=?", (ident,)).fetchone()
            if previous:
                if previous["digest"] != digest:
                    raise ValueError("event id reused with different content")
                return {"id": ident, "duplicate": True, "deliveries": 0}
            outstanding = db.execute(
                "SELECT COUNT(*) FROM deliveries WHERE status IN ('READY','INFLIGHT')"
            ).fetchone()[0]
            unexpired = db.execute(
                "SELECT COUNT(*) FROM events WHERE expires>?", (now,)
            ).fetchone()[0]
            if unexpired >= MAX_BACKLOG or outstanding + len(consumers) > MAX_BACKLOG:
                raise RuntimeError("cortex backlog full")
            db.execute("""INSERT INTO events
                (id,workspace,topic,source,severity,untrusted,sensitivity,payload_json,
                 digest,created,expires) VALUES (?,?,?,?,?,?,?,?,?,?,?)""",
                (ident, self.workspace, topic, source, severity, int(untrusted),
                 sensitivity, body, digest, now, now + ttl_seconds))
            db.executemany(
                "INSERT INTO deliveries(event_id,consumer) VALUES (?,?)",
                [(ident, consumer) for consumer in consumers])
        return {"id": ident, "duplicate": False, "deliveries": len(consumers)}

    def claim(self, consumer: str, limit: int = 10, lease_seconds: int = 60) -> list[dict]:
        if consumer not in TOPICS:
            raise ValueError("unknown consumer")
        if type(limit) is not int or not 1 <= limit <= 50:
            raise ValueError("limit must be 1..50")
        if type(lease_seconds) is not int or not 10 <= lease_seconds <= 300:
            raise ValueError("lease must be 10..300 seconds")
        now = time.time()
        results = []
        with self._db(write=True) as db:
            db.execute("""UPDATE deliveries SET status='DEAD',lease_token=NULL
                WHERE attempts>=? AND status='INFLIGHT' AND lease_until<=?""",
                (MAX_ATTEMPTS, now))
            rows = db.execute("""SELECT e.*,d.attempts FROM events e
                JOIN deliveries d ON d.event_id=e.id
                WHERE d.consumer=? AND e.expires>?
                  AND ((d.status='READY' AND d.next_at<=?)
                    OR (d.status='INFLIGHT' AND d.lease_until<=?))
                  AND d.attempts<?
                ORDER BY e.severity DESC,e.created ASC LIMIT ?""",
                (consumer, now, now, now, MAX_ATTEMPTS, limit)).fetchall()
            for row in rows:
                token = secrets.token_urlsafe(24)
                db.execute("""UPDATE deliveries SET status='INFLIGHT',
                    attempts=attempts+1,lease_token=?,lease_until=?
                    WHERE event_id=? AND consumer=?""",
                    (token, now + lease_seconds, row["id"], consumer))
                results.append({
                    "id": row["id"], "topic": row["topic"], "source": row["source"],
                    "severity": row["severity"], "untrusted": bool(row["untrusted"]),
                    "sensitivity": row["sensitivity"], "payload": json.loads(row["payload_json"]),
                    "created": row["created"], "expires": row["expires"],
                    "attempt": row["attempts"] + 1, "lease_token": token,
                })
        return results

    def settle(self, consumer: str, event_id: str, token: str, *, success: bool) -> bool:
        if consumer not in TOPICS or not isinstance(token, str) or len(token) < 20:
            raise ValueError("invalid consumer or token")
        with self._db(write=True) as db:
            row = db.execute("""SELECT status,attempts,lease_token,lease_until FROM deliveries
                    WHERE event_id=? AND consumer=?""", (event_id, consumer)).fetchone()
            if not row or row["status"] != "INFLIGHT" or row["lease_token"] != token:
                return False
            if row["lease_until"] <= time.time():
                return False
            if success:
                status, retry = "DONE", 0
            elif row["attempts"] >= MAX_ATTEMPTS:
                status, retry = "DEAD", 0
            else:
                status, retry = "READY", time.time() + min(300, 2 ** row["attempts"])
            db.execute("""UPDATE deliveries SET status=?,next_at=?,
                    lease_token=NULL,lease_until=NULL WHERE event_id=? AND consumer=?""",
                       (status, retry, event_id, consumer))
            return True

    def get_event(self, ident: str) -> dict | None:
        with self._db() as db:
            row = db.execute("SELECT * FROM events WHERE id=?", (ident,)).fetchone()
        if row is None:
            return None
        return {"id": row["id"], "topic": row["topic"], "source": row["source"],
                "severity": row["severity"], "untrusted": bool(row["untrusted"]),
                "sensitivity": row["sensitivity"], "expires": row["expires"]}

    def update_context(self, field: str, value: str, *, source: str = "user",
                       ttl_seconds: int = 3600, sensitivity: str = "private") -> dict:
        if field not in TWIN_FIELDS or not isinstance(value, str) or len(value) > 120:
            raise ValueError("invalid contextual field or value")
        if not value.strip():
            raise ValueError("contextual value cannot be empty")
        _name(source, "source")
        if sensitivity not in SENSITIVITIES:
            raise ValueError("invalid sensitivity")
        if type(ttl_seconds) is not int or not 30 <= ttl_seconds <= 86400:
            raise ValueError("invalid TTL")
        now = time.time()
        with self._db(write=True) as db:
            db.execute("""INSERT INTO context(field,value_json,source,sensitivity,observed,expires,revision)
                VALUES(?,?,?,?,?,?,1) ON CONFLICT(field) DO UPDATE SET
                value_json=excluded.value_json,source=excluded.source,
                sensitivity=excluded.sensitivity,observed=excluded.observed,
                expires=excluded.expires,revision=context.revision+1""",
                (field, _json(value), source, sensitivity, now, now + ttl_seconds))
        return {"field": field, "updated": True}

    def context(self) -> dict:
        now = time.time()
        with self._db() as db:
            rows = db.execute("""SELECT field,value_json,source,sensitivity,observed,expires,revision
                              FROM context WHERE expires>?""", (now,)).fetchall()
        return {row["field"]: {
            "value": json.loads(row["value_json"]), "source": row["source"],
            "sensitivity": row["sensitivity"], "observed": row["observed"],
            "expires": row["expires"], "revision": row["revision"],
        } for row in rows}

    def status(self) -> dict:
        with self._db() as db:
            counts = db.execute("""SELECT consumer,status,COUNT(*) AS total FROM deliveries
                GROUP BY consumer,status""").fetchall()
            latest = db.execute("SELECT COUNT(*) FROM events WHERE expires>?", (time.time(),)).fetchone()[0]
        return {"workspace": self.workspace, "unexpired_events": latest,
                "deliveries": [dict(x) for x in counts], "consumers": sorted(TOPICS)}

    def prune(self) -> int:
        now = time.time()
        with self._db(write=True) as db:
            count = db.execute("DELETE FROM events WHERE expires<=?", (now,)).rowcount
            db.execute("DELETE FROM context WHERE expires<=?", (now,))
        return count


def preflight_action(event: dict | None, *, effect: str, tool_domain: str) -> dict:
    """Advisory only. The Agent must *still* apply its real policy and approvals.

    No model can grant execution authority. Unknown effects fail closed.
    """
    _name(tool_domain, "tool_domain")
    if event is None:
        return {"tier": "RED", "decision": "DENY", "reason": "missing event provenance"}
    if effect not in {"pure", "read", "write", "destructive", "external_side_effect", "privileged"}:
        return {"tier": "RED", "decision": "DENY", "reason": "unknown effect"}
    if event.get("expires", 0) <= time.time():
        return {"tier": "RED", "decision": "DENY", "reason": "event expired"}
    if effect in {"destructive", "privileged"}:
        return {"tier": "RED", "decision": "DENY", "reason": "forbidden autonomous effect"}
    event_domain = str(event.get("topic", "")).split(".", 1)[0]
    if effect in {"write", "external_side_effect"}:
        return {"tier": "YELLOW", "decision": "REQUIRE_APPROVAL",
                "reason": "side effect requires existing Agent approval"}
    if event.get("untrusted") and tool_domain != event_domain:
        return {"tier": "YELLOW", "decision": "REQUIRE_APPROVAL",
                "reason": "cross-domain access from untrusted event"}
    return {"tier": "GREEN", "decision": "READ_ONLY",
            "reason": "read-only preflight; Agent permissions still apply"}
