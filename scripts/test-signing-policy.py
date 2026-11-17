#!/usr/bin/env python3
"""Regression tests for scripts/signing-policy.py."""
import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location("signing_policy", Path(__file__).with_name("signing-policy.py"))
policy = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(policy)

PUBLIC = "public-key"
PREVIOUS_SIGNED = {"updateSigning": "provisioned", "updaterPublicKey": PUBLIC}


class SigningPolicyTest(unittest.TestCase):
    def test_initial_unsigned_is_allowed(self):
        self.assertFalse(policy.evaluate(None, False, "", False))

    def test_first_signed_release_is_allowed(self):
        self.assertTrue(policy.evaluate(None, False, PUBLIC, True))

    def test_continued_signed_release_is_allowed(self):
        self.assertTrue(policy.evaluate(PREVIOUS_SIGNED, True, PUBLIC, True))

    def test_signed_channel_cannot_drop_the_key(self):
        with self.assertRaisesRegex(ValueError, "required"):
            policy.evaluate(PREVIOUS_SIGNED, True, "", False)

    def test_signed_channel_cannot_change_the_key(self):
        with self.assertRaisesRegex(ValueError, "differs"):
            policy.evaluate(PREVIOUS_SIGNED, True, "other-key", True)

    def test_partial_current_key_pair_fails(self):
        with self.assertRaisesRegex(ValueError, "missing"):
            policy.evaluate(None, False, PUBLIC, False)
        with self.assertRaisesRegex(ValueError, "missing"):
            policy.evaluate(None, False, "", True)

    def test_manifest_without_metadata_key_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "no updaterPublicKey"):
            policy.evaluate({"updateSigning": "not-provisioned"}, True, PUBLIC, True)

    def test_inconsistent_previous_metadata_fails_closed(self):
        with self.assertRaisesRegex(ValueError, "not marked signed"):
            policy.evaluate({"updateSigning": "not-provisioned", "updaterPublicKey": PUBLIC}, False, PUBLIC, True)


if __name__ == "__main__":
    unittest.main()
