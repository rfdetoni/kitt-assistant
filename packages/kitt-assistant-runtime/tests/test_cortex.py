"""Cortex contracts; no network, cloud credentials, or Agent execution required."""
from __future__ import annotations

import json
from pathlib import Path
import tempfile
import time
import unittest

from kitt.cortex import CortexStore, preflight_action, advise_route


class CortexContractTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        home = Path(self.tmp.name)
        self.store = CortexStore(home / "repo-a", data_dir=home / "state")

    def publish(self, **overrides):
        data = dict(topic="saas.error.500", source="external.webhook",
                    payload={"service": "billing"}, severity=4,
                    sensitivity="private", event_id="incident-42", untrusted=True)
        data.update(overrides)
        return self.store.publish(**data)

    def test_fanout_and_durable_reopen(self):
        published = self.publish()
        self.assertEqual(1, published["deliveries"])
        reopened = CortexStore(self.store.workspace, data_dir=Path(self.tmp.name) / "state")
        claimed = reopened.claim("plugin-saas-sre", 2)
        self.assertEqual(1, len(claimed))
        self.assertTrue(claimed[0]["untrusted"])
        self.assertEqual("saas.error.500", claimed[0]["topic"])
        self.assertFalse(reopened.claim("plugin-home-sec"))
        self.assertTrue(reopened.settle(
            "plugin-saas-sre", claimed[0]["id"], claimed[0]["lease_token"], success=True))
        self.assertEqual([], reopened.claim("plugin-saas-sre"))

    def test_idempotent_delivery_and_conflicting_replay(self):
        first = self.publish()
        second = self.publish()
        self.assertEqual(first["id"], second["id"])
        self.assertTrue(second["duplicate"])
        with self.assertRaisesRegex(ValueError, "reused"):
            self.publish(payload={"service": "different"})
        self.assertEqual(1, len(self.store.claim("plugin-saas-sre")))

    def test_lease_is_single_use_and_retries_with_backoff(self):
        self.publish()
        first = self.store.claim("plugin-saas-sre")[0]
        self.assertFalse(self.store.settle(
            "plugin-saas-sre", first["id"], "invalid", success=True))
        self.assertTrue(self.store.settle(
            "plugin-saas-sre", first["id"], first["lease_token"], success=False))
        self.assertEqual([], self.store.claim("plugin-saas-sre"))
        self.assertFalse(self.store.settle(
            "plugin-saas-sre", first["id"], first["lease_token"], success=True))

    def test_validation_and_bounded_input(self):
        with self.assertRaises(ValueError):
            self.publish(severity=True)
        with self.assertRaises(ValueError):
            self.publish(payload={"body": "x" * (16 * 1024)})
        with self.assertRaises(ValueError):
            self.publish(topic="../external")
        with self.assertRaises(ValueError):
            self.publish(ttl_seconds=1)
        with self.assertRaises(ValueError):
            self.store.claim("arbitrary-plugin")
        with self.assertRaises(ValueError):
            self.store.claim("plugin-saas-sre", 5000)

    def test_event_provenance_and_governor(self):
        self.publish()
        event = self.store.get_event("incident-42")
        self.assertEqual("YELLOW", preflight_action(
            event, effect="read", tool_domain="repo")["tier"])
        self.assertEqual("GREEN", preflight_action(
            event, effect="read", tool_domain="saas")["tier"])
        self.assertEqual("YELLOW", preflight_action(
            event, effect="write", tool_domain="saas")["tier"])
        self.assertEqual("RED", preflight_action(
            event, effect="privileged", tool_domain="saas")["tier"])
        self.assertEqual("DENY", preflight_action(
            None, effect="read", tool_domain="saas")["decision"])
        self.assertEqual("DENY", preflight_action(
            event, effect="unknown", tool_domain="saas")["decision"])

    def test_context_ttl_and_revisions(self):
        self.store.update_context("activity_state", "DEEP_WORK", ttl_seconds=30)
        self.assertEqual("DEEP_WORK", self.store.context()["activity_state"]["value"])
        self.store.update_context("activity_state", "IDLE", ttl_seconds=30)
        self.assertEqual(2, self.store.context()["activity_state"]["revision"])
        with self.assertRaises(ValueError):
            self.store.update_context("daily_limit_usd", "9999")
        with self.store._db(write=True) as db:
            db.execute("UPDATE context SET expires=? WHERE field='activity_state'",
                       (time.time() - 1,))
        self.assertNotIn("activity_state", self.store.context())
        self.assertEqual(0, self.store.prune())

    def test_workspaces_do_not_share_state(self):
        self.publish()
        other = CortexStore(Path(self.tmp.name) / "repo-b",
                            data_dir=Path(self.tmp.name) / "state")
        self.assertNotEqual(self.store.path, other.path)
        self.assertFalse(other.claim("plugin-saas-sre"))
        self.assertEqual(0, other.status()["unexpired_events"])

    def test_decision_advisor_fails_closed_and_cannot_choose_remote(self):
        self.publish()
        event = self.store.get_event("incident-42")
        result = advise_route(event, endpoint="http://169.254.169.254",
                              model="a-decision-model")
        self.assertEqual("invalid_endpoint", result["status"])
        self.assertEqual("local_only", result["route"])
        result = advise_route(event, endpoint="", model="")
        self.assertEqual("local_only", result["route"])
        # Decision models only classify; they do not execute a tool or grant permission.
        self.assertIsNone(result["advisory"])


if __name__ == "__main__":
    unittest.main()
