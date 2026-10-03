#!/usr/bin/env python3
"""Exercise handover against every real production SQLite migration."""
import copy
import json
from pathlib import Path
import sqlite3
import unittest
import uuid

from adopt_legacy_reviews import encoded, finalize, prepare, validate


class ReviewHandoverTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        for migration in sorted((Path(__file__).resolve().parents[1] / "crates/server/migrations").glob("*.sql")):
            self.db.executescript(migration.read_text())
        record = dict(app_id="ring", org_id="tos", application_id="ring", app_name="Silicon Ring",
                      app_logo=None, base_url="https://ring.example.com", configuration_revision=0,
                      iam_revision=6, credential_version=2, availability="verified", visibility="private",
                      testing_idle_days=30, app_scope={"iam": ["self.identity.read", "directory.carbons.read"],
                      "external": [{"app_id": "ting", "endpoint_id": "send"}]},
                      effective_scopes=[{"scope": "self.identity.read"}], obo_endpoints=[],
                      obo_review_message=None, webhook_scope=["full"], webhook_url="https://ring.example.com/webhook", pending_webhook_url=None)
        config = dict(org_id="tos", app_id="ring", name="Silicon Ring", description=" ".join(["description"] * 50),
                      logo_url=None, base_url=record["base_url"], app_scope=record["app_scope"],
                      webhook_url="https://ring.example.com/webhook", webhook_scope=["full"],
                      obo_endpoints=[], obo_review_message=None, testing_idle_days=30, visibility="public")
        self.source = dict(schema="honeycomb.legacy-review-handover.v1", application=record,
                           configuration=config, webhook_secret="never-copy-plaintext", requests=[])
        for provider, scopes in [("iam", ["directory.carbons.read"]), ("ting", ["obo:ting:send"])]:
            self.source["requests"].append(dict(id=str(uuid.uuid4()), app_id="ring", provider=provider,
                scopes=scopes, status="pending", requested_by="c:author", created_at=123, updated_at=456,
                messages=[dict(id=str(uuid.uuid4()), actor="c:author", message="Original justification", created_at=123),
                          dict(id=str(uuid.uuid4()), actor="c:reviewer", message="Original follow-up", created_at=456)]))
        self.digest = "a" * 64

    def prepare(self):
        with self.db:
            return prepare(self.db, self.source, self.digest, "operator:test", lambda value: "encrypted-test:" + str(len(value)))

    def plan(self):
        request = self.db.execute("SELECT id FROM publication_requests").fetchone()[0]
        plan = dict(state="accepted", request_id=request, plan_id=str(uuid.uuid4()), app_id="ring",
                    configuration_revision=1, visibility="public",
                    gates=[dict(provider=r["provider"], scopes=r["scopes"]) for r in self.source["requests"]] +
                          [dict(provider="honeycomb", scopes=[])])
        with self.db:
            self.db.execute("UPDATE publication_requests SET state='awaiting_scope_review',plan_id=? WHERE id=?", (plan["plan_id"], request))
            for gate in plan["gates"]:
                self.db.execute("INSERT INTO review_gates(request_id,provider,scopes) VALUES(?,?,?)", (request, gate["provider"], encoded(gate["scopes"])))
        return plan

    def finish(self, plan):
        with self.db:
            return finalize(self.db, self.source, self.digest, "operator:test", plan)

    def test_prepare_preserves_discussions_and_effective_grants_without_publication(self):
        receipt = self.prepare()
        app = self.db.execute("SELECT revision,iam_revision,effective_revision,credential_version,visibility,config,effective_config,webhook_secret FROM applications").fetchone()
        self.assertEqual(app[:5], (1, 6, 0, 2, "private"))
        self.assertEqual(json.loads(app[5])["app_scope"], self.source["application"]["app_scope"])
        effective = json.loads(app[6])
        self.assertEqual(effective["app_scope"], {"iam": ["self.identity.read"], "external": []})
        self.assertEqual(effective["visibility"], "private")
        self.assertNotIn("webhook_url", effective)
        self.assertNotIn("never-copy-plaintext", "\n".join(self.db.iterdump()))
        self.assertEqual(self.db.execute("SELECT state,requested_by,created_at FROM publication_requests").fetchone(), ("awaiting_review_plan", "c:author", 123))
        for request in self.source["requests"]:
            for message in request["messages"]:
                self.assertEqual(self.db.execute("SELECT request_id,actor,provider,message,created_at FROM discussions WHERE id=?", (message["id"],)).fetchone(),
                                 (receipt["request_id"], message["actor"], request["provider"], message["message"], message["created_at"]))
        for table in ["outbox", "decisions", "releases", "review_gates", "legacy_review_requests", "publication_authorizations", "request_reads"]:
            self.assertEqual(self.db.execute("SELECT COUNT(*) FROM " + table).fetchone()[0], 0, table)
        again = self.prepare()
        self.assertTrue(again["replayed"])
        self.assertEqual(again["request_id"], receipt["request_id"])
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM discussions").fetchone()[0], 4)

    def test_finalize_requires_real_exact_plan_and_preserves_pending_state(self):
        receipt = self.prepare()
        plan = self.plan()
        self.assertEqual(self.finish(plan)["state"], "complete")
        self.assertTrue(self.finish(plan)["replayed"])
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM legacy_review_requests").fetchone()[0], 2)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM audit WHERE action='iam.review_handover.complete'").fetchone()[0], 1)
        for request in self.source["requests"]:
            self.assertEqual(self.db.execute("SELECT request_id,provider,source_sha256 FROM legacy_review_requests WHERE legacy_request_id=?", (request["id"],)).fetchone(), (receipt["request_id"], request["provider"], self.digest))
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM review_gates WHERE state='pending' AND decision_id IS NULL").fetchone()[0], 3)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM decisions").fetchone()[0], 0)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM outbox").fetchone()[0], 0)

    def test_public_under_review_source_stays_disabled_private_without_minting_credentials(self):
        self.source["application"].update(availability="under_review", visibility="public", credential_version=None,
                                          webhook_url=None, pending_webhook_url="https://ring.example.com/webhook")
        self.prepare()
        self.finish(self.plan())
        self.assertEqual(self.db.execute("SELECT visibility,state,credential_version,effective_revision FROM applications").fetchone(), ("private", "disabled", 0, 0))
        self.assertEqual(self.source["application"]["visibility"], "public")
        self.assertIsNone(self.source["application"]["credential_version"])

    def test_precise_source_timestamps_remain_in_provenance(self):
        for request in self.source["requests"]:
            request.update(created_at="2026-10-03T14:46:34.652092+00:00", updated_at="2026-10-03T14:50:40.560575+00:00")
            request["messages"][0]["created_at"] = request["created_at"]
            request["messages"][1]["created_at"] = request["updated_at"]
        self.prepare()
        self.finish(self.plan())
        stored = json.loads(self.db.execute("SELECT source_requests FROM legacy_review_handovers").fetchone()[0])
        self.assertEqual(stored, self.source["requests"])

    def test_finalize_rejects_changed_plan_grants_application_and_discussion(self):
        self.prepare()
        plan = self.plan()
        for field, value in [("state", "pending"), ("app_id", "other"), ("configuration_revision", 2), ("request_id", str(uuid.uuid4())), ("gates", [{"provider": "honeycomb", "scopes": []}])]:
            with self.subTest(field=field), self.assertRaises(ValueError):
                self.finish(dict(plan, **{field: value}))
        for sql in ["UPDATE applications SET credential_version=3", "UPDATE applications SET revision=2",
                    "UPDATE applications SET visibility='public'", "UPDATE review_gates SET state='approved' WHERE provider='iam'",
                    "UPDATE discussions SET message='Changed'", "UPDATE publication_requests SET state='awaiting_activation'"]:
            with self.subTest(sql=sql):
                self.db.execute("SAVEPOINT changed")
                self.db.execute(sql)
                with self.assertRaises(ValueError):
                    finalize(self.db, self.source, self.digest, "operator:test", plan)
                self.db.execute("ROLLBACK TO changed")
                self.db.execute("RELEASE changed")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM legacy_review_requests").fetchone()[0], 0)

    def test_prepare_rejects_existing_app_and_changed_source_without_overwrite(self):
        self.prepare()
        with self.assertRaises(ValueError), self.db:
            prepare(self.db, self.source, "b" * 64, "operator:test", lambda value: value)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM applications").fetchone()[0], 1)

    def test_export_validation_rejects_state_scope_identity_and_plaintext_fields(self):
        mutations = [lambda s: s["application"].update(visibility="public"),
                     lambda s: s["application"].update(configuration_revision=1),
                     lambda s: s["configuration"].update(name="Other app"),
                     lambda s: s["configuration"].update(webhook_secret="exposed"),
                     lambda s: s["configuration"].update(webhook_url="https://different.example.com/webhook"),
                     lambda s: s["application"].update(webhook_url=None),
                     lambda s: s["application"].update(availability="under_review", visibility="public", pending_webhook_url="https://different.example.com/webhook"),
                     lambda s: s["requests"][0].update(status="approved"),
                     lambda s: s["requests"][0].update(scopes=["obo:ting:send"]),
                     lambda s: s["requests"][0].update(requested_by="arbitrary-actor"),
                     lambda s: s["requests"][0]["messages"][0].update(created_at=999),
                     lambda s: s["requests"].append(copy.deepcopy(s["requests"][0]))]
        for mutation in mutations:
            source = copy.deepcopy(self.source)
            mutation(source)
            with self.subTest(source=source["requests"][0]["id"]), self.assertRaises(ValueError):
                validate(source)


if __name__ == "__main__":
    unittest.main()
