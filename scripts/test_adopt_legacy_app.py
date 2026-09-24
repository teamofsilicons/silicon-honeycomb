#!/usr/bin/env python3
"""Exercise legacy import against the complete production SQLite schema."""
import json
from pathlib import Path
import sqlite3
import unittest
from adopt_legacy_app import insert, projection


class AdoptionTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(":memory:")
        for migration in sorted((Path(__file__).resolve().parents[1] / "crates/server/migrations").glob("*.sql")):
            self.db.executescript(migration.read_text())
        self.record = dict(app_id="briefcase", org_id="tos", application_id="existing-id",
                           app_name="Silicon Briefcase", app_logo=None, base_url="https://example.com",
                           configuration_revision=0, iam_revision=9, credential_version=1,
                           availability="verified", visibility="public", testing_idle_days=30,
                           app_scope={"iam": ["granted", "pending"], "external": []},
                           effective_scopes=[{"scope": "granted"}], obo_endpoints=[],
                           obo_review_message=None, webhook_scope=["full"], app_secret="MUST-NOT-COPY")
        self.metadata = {"org_id": "tos", "description": " ".join(["description"] * 50), "website_url": "https://example.com"}

    def config(self):
        return projection(self.record, "briefcase", "existing-id", self.metadata)

    def test_import_preserves_identity_and_filters_secrets_pending_grants_and_publication(self):
        config = self.config()
        with self.db:
            receipt = insert(self.db, self.record, config, "operator:test")
        app = self.db.execute("SELECT org_id,revision,iam_revision,effective_revision,credential_version,visibility,state,webhook_secret,config FROM applications").fetchone()
        self.assertEqual(app[:8], ("tos", 0, 9, 0, 1, "private", "active", ""))
        self.assertEqual(json.loads(app[8])["app_scope"]["iam"], ["granted"])
        self.assertNotIn("webhook_url", config)
        self.assertNotIn("MUST-NOT-COPY", json.dumps(config))
        self.assertEqual(receipt["application_id"], "existing-id")
        self.assertEqual(self.db.execute("SELECT action FROM audit").fetchone()[0], "iam.adopt")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM publication_requests").fetchone()[0], 0)
        with self.assertRaises(sqlite3.IntegrityError), self.db:
            insert(self.db, self.record, config, "operator:duplicate")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM operations").fetchone()[0], 1)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM applications").fetchone()[0], 1)

    def test_mismatched_or_unaccepted_record_is_rejected(self):
        for field, value in [("org_id", "other"), ("application_id", "other-id"),
                             ("app_id", "other"), ("configuration_revision", 1),
                             ("iam_revision", 0), ("availability", "disabled"), ("credential_version", 0)]:
            with self.subTest(field=field):
                record = dict(self.record, **{field: value})
                with self.assertRaises(ValueError):
                    projection(record, "briefcase", "existing-id", self.metadata)

    def test_explicit_public_adoption_preserves_declarations_but_not_pending_grants(self):
        self.record["application_id"] = "briefcase"
        endpoint = {"id": "files.read", "critical": True, "enabled": True,
                    "metadata": {"existing": True}, "access_proof_ttl_seconds": 60}
        self.record["obo_endpoints"] = [endpoint]
        self.record["app_scope"]["external"] = [
            {"app_id": "provider", "endpoint_id": "granted"},
            {"app_id": "provider", "endpoint_id": "pending"},
        ]
        self.record["effective_scopes"].append({"scope": "obo:provider:granted"})
        config = projection(self.record, "briefcase", "briefcase", self.metadata, True)
        with self.db:
            receipt = insert(self.db, self.record, config, "operator:test", True)
        app = self.db.execute("SELECT visibility,config,effective_config,webhook_secret FROM applications").fetchone()
        desired, effective = json.loads(app[1]), json.loads(app[2])
        self.assertEqual(app[0], "public")
        self.assertEqual(app[3], "")
        self.assertEqual(desired["visibility"], "public")
        self.assertEqual(desired["app_scope"], self.record["app_scope"])
        self.assertEqual(desired["obo_endpoints"], [endpoint])
        self.assertEqual(effective["app_scope"]["iam"], ["granted"])
        self.assertEqual(effective["app_scope"]["external"], self.record["app_scope"]["external"][:1])
        self.assertNotIn("MUST-NOT-COPY", json.dumps([desired, effective, receipt]))
        self.assertTrue(receipt["preserved_public_visibility"])
        self.assertEqual(receipt["visibility"], "public")
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM publication_requests").fetchone()[0], 0)
        stored = json.loads(self.db.execute("SELECT result FROM operations WHERE kind='iam.adopt'").fetchone()[0])
        self.assertEqual(stored, receipt)

    def test_public_adoption_requires_current_verified_public_revision_zero_identity(self):
        for field, value in [("visibility", "private"), ("availability", "disabled"),
                             ("configuration_revision", 1), ("configuration_revision", False), ("iam_revision", 0),
                             ("iam_revision", True), ("credential_version", 0),
                             ("credential_version", True)]:
            with self.subTest(field=field, value=value):
                record = dict(self.record, **{field: value})
                with self.assertRaises(ValueError):
                    projection(record, "briefcase", "existing-id", self.metadata, True)
                with self.assertRaises(ValueError):
                    insert(self.db, record, self.config(), "operator:test", True)
        self.assertEqual(self.db.execute("SELECT COUNT(*) FROM applications").fetchone()[0], 0)


if __name__ == "__main__":
    unittest.main()
