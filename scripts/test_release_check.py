#!/usr/bin/env python3
"""Exercise release guards that must stop an incorrect publication."""

from copy import deepcopy
import unittest

from release_check import metadata, validate


class ReleaseValidation(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.data = metadata()

    def setUp(self):
        self.candidate = deepcopy(self.data)

    def package(self, name):
        return next(p for p in self.candidate["packages"] if p["name"] == name)

    def test_wrong_requested_version_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "Requested"):
            validate(self.candidate, "9.9.9")

    def test_divergent_crate_version_is_rejected(self):
        self.package("rayengine-cli")["version"] = "9.9.9"
        with self.assertRaisesRegex(ValueError, "share release version"):
            validate(self.candidate)

    def test_accidentally_publishable_example_is_rejected(self):
        self.package("rayengine-minecraft")["publish"] = None
        with self.assertRaisesRegex(ValueError, "Expected publishable crates"):
            validate(self.candidate)

    def test_stale_internal_dependency_is_rejected(self):
        dependency = next(d for d in self.package("rayengine")["dependencies"] if d["name"] == "rayengine-core")
        dependency["req"] = "^0.0.0"
        with self.assertRaisesRegex(ValueError, "internal dependency"):
            validate(self.candidate)


if __name__ == "__main__":
    unittest.main()
