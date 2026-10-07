"""Tests for the building kit's --check comparison: python3 -m unittest discover -s tools"""

import struct
import unittest

from generate_buildings import close, glb, glb_content, make, same_glb


def nudged(glb_bytes, factor):
    """The GLB with every binary float scaled by `factor`, as another libm might round it."""
    text_length = struct.unpack_from("<I", glb_bytes, 12)[0]
    head, data = glb_bytes[:28 + text_length], glb_bytes[28 + text_length:]
    floats = struct.unpack(f"<{len(data) // 4}f", data)
    return head + struct.pack(f"<{len(floats)}f", *(f * factor for f in floats))


class SameGlb(unittest.TestCase):
    def setUp(self):
        self.guard_post = glb(make("guard_post"))

    def test_decodes_document_and_floats(self):
        doc, floats = glb_content(self.guard_post)
        self.assertEqual([m["name"] for m in doc["meshes"]], ["Body", "Scaffold"])
        self.assertEqual(len(floats) * 4, doc["buffers"][0]["byteLength"])

    def test_tolerates_rounding(self):
        self.assertTrue(same_glb(self.guard_post, nudged(self.guard_post, 1 + 1e-7)))

    def test_rejects_moved_geometry(self):
        self.assertFalse(same_glb(self.guard_post, nudged(self.guard_post, 1.01)))

    def test_rejects_another_model(self):
        self.assertFalse(same_glb(self.guard_post, glb(make("vault"))))

    def test_close_keeps_strings_and_structure_exact(self):
        self.assertTrue(close({"a": [0.5, "x"]}, {"a": [0.500001, "x"]}))
        self.assertFalse(close({"a": [0.5, "x"]}, {"a": [0.5, "y"]}))
        self.assertFalse(close([1.0], [1.0, 2.0]))


if __name__ == "__main__":
    unittest.main()
