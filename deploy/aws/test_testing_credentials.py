import unittest
from testing_credentials import TOKEN, desired_credentials


class TestingCredentials(unittest.TestCase):
    def test_generated_once_and_preserves_unrelated_configuration(self):
        hc = {"backend": {"HONEYCOMB_ENCRYPTION_KEY": "keep"}, "library": {"keep": True}}
        bc = {"BRIEFCASE_IAM_APP_SECRET": "keep"}
        h, b = desired_credentials(hc, bc)
        self.assertGreaterEqual(len(b[TOKEN]), 32)
        self.assertEqual(h["backend"][TOKEN], b[TOKEN])
        self.assertEqual(desired_credentials(h, b), (h, b))
        self.assertEqual(h["library"], hc["library"])
        self.assertEqual(b["BRIEFCASE_IAM_APP_SECRET"], "keep")
        self.assertNotIn(TOKEN, bc)
        self.assertNotIn(TOKEN, hc["backend"])

    def test_recovers_partial_setup_without_rotation(self):
        token = "preserve-existing-service-token-1234567890"
        for hc, bc in [({"backend": {TOKEN: token}}, {}), ({"backend": {}}, {TOKEN: token})]:
            h, b = desired_credentials(hc, bc)
            self.assertEqual(h["backend"][TOKEN], token)
            self.assertEqual(b[TOKEN], token)

    def test_conflicting_credentials_fail_without_overwrite(self):
        with self.assertRaisesRegex(ValueError, "differ"):
            desired_credentials({"backend": {TOKEN: "a" * 48}}, {TOKEN: "b" * 48})

    def test_invalid_existing_credentials_are_not_silently_replaced(self):
        for token in ["short", "x" * 40 + "\n", "é" * 40, 1234]:
            with self.subTest(token=type(token).__name__), self.assertRaises(ValueError):
                desired_credentials({"backend": {}}, {TOKEN: token})

    def test_custom_service_origins_are_preserved(self):
        h, b = desired_credentials({"backend": {"BRIEFCASE_BASE_URL": "https://briefcase.example"}},
                                  {"BRIEFCASE_HONEYCOMB_BASE_URL": "https://honeycomb.example"})
        self.assertEqual(h["backend"]["BRIEFCASE_BASE_URL"], "https://briefcase.example")
        self.assertEqual(b["BRIEFCASE_HONEYCOMB_BASE_URL"], "https://honeycomb.example")


if __name__ == "__main__":
    unittest.main()
