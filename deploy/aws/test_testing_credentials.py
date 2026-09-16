import copy
import json
import unittest
from testing_credentials import REGISTRY, TOKEN, desired_credentials


def briefcase(**extra):
    return {
        "BRIEFCASE_IAM_APP_ID": "vendor>storage",
        "BRIEFCASE_PUBLIC_BASE_URL": "https://storage.example/api/v1/",
        "BRIEFCASE_HONEYCOMB_BASE_URL": "https://packages.example/",
        **extra,
    }


class TestingCredentials(unittest.TestCase):
    def test_generated_once_and_preserves_unrelated_configuration(self):
        hc = {"backend": {"HONEYCOMB_ENCRYPTION_KEY": "keep"}, "library": {"keep": True}}
        bc = briefcase(BRIEFCASE_IAM_APP_SECRET="keep")
        h, b = desired_credentials(hc, bc)
        self.assertGreaterEqual(len(b[TOKEN]), 32)
        self.assertEqual(h["backend"][TOKEN], b[TOKEN])
        self.assertEqual(desired_credentials(h, b), (h, b))
        self.assertEqual(h["library"], hc["library"])
        self.assertEqual(b["BRIEFCASE_IAM_APP_SECRET"], "keep")
        self.assertNotIn(TOKEN, bc)
        self.assertNotIn(TOKEN, hc["backend"])
        self.assertEqual(h["backend"]["BRIEFCASE_APP_ID"], "vendor>storage")
        self.assertEqual(h["backend"]["BRIEFCASE_BASE_URL"], "https://storage.example")
        self.assertEqual(json.loads(h["backend"][REGISTRY]), [{
            "app_id": "vendor>storage", "base_url": "https://storage.example", "token_env": TOKEN,
        }])

    def test_recovers_partial_setup_without_rotation(self):
        token = "preserve-existing-service-token-1234567890"
        for hc, bc in [({"backend": {TOKEN: token}}, briefcase()),
                       ({"backend": {}}, briefcase(**{TOKEN: token}))]:
            h, b = desired_credentials(hc, bc)
            self.assertEqual(h["backend"][TOKEN], token)
            self.assertEqual(b[TOKEN], token)

    def test_conflicting_credentials_fail_without_overwrite(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            desired_credentials({"backend": {TOKEN: "a" * 48}}, briefcase(**{TOKEN: "b" * 48}))

    def test_invalid_existing_credentials_are_not_silently_replaced(self):
        for token in ["short", "x" * 40 + "\n", "é" * 40, 1234]:
            with self.subTest(token=type(token).__name__), self.assertRaises(ValueError):
                desired_credentials({"backend": {}}, briefcase(**{TOKEN: token}))

    def test_custom_service_origins_are_preserved(self):
        h, b = desired_credentials({"backend": {"BRIEFCASE_BASE_URL": "https://storage.example/"}},
                                  briefcase(BRIEFCASE_HONEYCOMB_BASE_URL="https://honeycomb.example"))
        self.assertEqual(h["backend"]["BRIEFCASE_BASE_URL"], "https://storage.example/")
        self.assertEqual(b["BRIEFCASE_HONEYCOMB_BASE_URL"], "https://honeycomb.example")

    def test_preserves_other_participants_and_resumes_without_registry_changes(self):
        other = {"app_id": "other>worker", "base_url": "https://worker.example", "token_env": "WORKER_TOKEN"}
        hc = {"backend": {REGISTRY: json.dumps([other]), "WORKER_TOKEN": "keep"}}
        updated, bc = desired_credentials(hc, briefcase())
        self.assertEqual(json.loads(updated["backend"][REGISTRY])[0], other)
        self.assertEqual(updated["backend"]["WORKER_TOKEN"], "keep")
        self.assertEqual(desired_credentials(updated, bc), (updated, bc))
        self.assertEqual(json.loads(hc["backend"][REGISTRY]), [other])

    def test_rejects_mismatched_identity_destination_and_token_assignment(self):
        for backend in [
            {"BRIEFCASE_APP_ID": "old>storage"},
            {"BRIEFCASE_BASE_URL": "https://old.example"},
            {REGISTRY: json.dumps([{"app_id": "vendor>storage", "base_url": "https://old.example", "token_env": TOKEN}])},
            {REGISTRY: json.dumps([{"app_id": "vendor>storage", "base_url": "https://storage.example", "token_env": "OLD_TOKEN"}])},
            {REGISTRY: json.dumps([{"app_id": "old>storage", "base_url": "https://storage.example", "token_env": TOKEN}])},
        ]:
            hc = {"backend": backend}
            previous = copy.deepcopy(hc)
            with self.subTest(backend=backend), self.assertRaises(ValueError):
                desired_credentials(hc, briefcase())
            self.assertEqual(hc, previous)

    def test_requires_explicit_identity_api_and_honeycomb_url(self):
        for field in ["BRIEFCASE_IAM_APP_ID", "BRIEFCASE_PUBLIC_BASE_URL", "BRIEFCASE_HONEYCOMB_BASE_URL"]:
            bc = briefcase()
            del bc[field]
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, field):
                desired_credentials({"backend": {}}, bc)
        for value in ["http://storage.example/api/v1/", "https://user:pass@storage.example/", "https://storage.example/?token=secret", "https://storage.example/#fragment", "https://storage.example:bad", "https://storage.example/ bad"]:
            with self.subTest(value=value), self.assertRaises(ValueError):
                desired_credentials({"backend": {}}, briefcase(BRIEFCASE_PUBLIC_BASE_URL=value))

    def test_explicit_runtime_url_is_supported_when_injected_outside_secret_store(self):
        bc = briefcase()
        del bc["BRIEFCASE_PUBLIC_BASE_URL"]
        hc, updated_bc = desired_credentials({"backend": {}}, bc, "https://configured.example/api/v1/")
        self.assertEqual(hc["backend"]["BRIEFCASE_BASE_URL"], "https://configured.example")
        self.assertNotIn("BRIEFCASE_PUBLIC_BASE_URL", updated_bc)
        with self.assertRaisesRegex(ValueError, "origins differ"):
            desired_credentials({"backend": {}}, briefcase(), "https://other.example/api/v1/")

    def test_invalid_or_duplicate_registry_is_not_overwritten(self):
        item = {"app_id": "vendor>storage", "base_url": "https://storage.example", "token_env": TOKEN}
        for registry in ["invalid", "{}", json.dumps([item, item]), json.dumps([{"app_id": "unknown"}])]:
            with self.subTest(registry=registry), self.assertRaises(ValueError):
                desired_credentials({"backend": {REGISTRY: registry}}, briefcase())


if __name__ == "__main__":
    unittest.main()
