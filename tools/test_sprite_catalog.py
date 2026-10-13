import unittest

from sprite_catalog import CATALOG, HFX, POINT, alpha_colours, animation, is_alpha, page


class CatalogTest(unittest.TestCase):
    def test_each_sprite_named_once(self):
        ids = [(f, i) for f, i, *_ in CATALOG]
        self.assertEqual(len(ids), len(set(ids)))

    def test_known_indices(self):
        names = {(f, i): n for f, i, _, n, _ in CATALOG}
        self.assertEqual(names[(HFX, 1058)], "Blast")
        self.assertEqual(names[(HFX, 1069)], "Angel of Death")
        self.assertEqual(names[(HFX, 1074)], "Teleport")
        self.assertEqual(names[(HFX, 1083)], "camp fire")
        self.assertEqual(names[(POINT, 57)], "Teleport")
        self.assertEqual(names[(POINT, 66)], "camp fire")

    def test_alpha_ranges(self):
        self.assertTrue(is_alpha(HFX, 1090) and is_alpha(HFX, 1592))
        self.assertFalse(is_alpha(HFX, 1500) or is_alpha(POINT, 1100))

    def test_alpha_colours_tint_and_strength(self):
        pal = [(0, 0, 0)] * 256
        pal[1] = (255, 255, 255)
        table = [0] * 65536
        table[(1 * 16 + 15) * 256:(2 * 16) * 256] = [1] * 256
        colours = alpha_colours(table, pal)
        self.assertEqual(colours[1 * 16 + 15], (255, 255, 255, 255))
        self.assertEqual(colours[1 * 16], (255, 255, 255, 0))
        self.assertEqual(colours[2 * 16 + 5], (0, 0, 0, 85))

    def test_animation_of_frames(self):
        self.assertEqual(animation(POINT, 86, "fight 3/6"), f"{POINT}:84")
        self.assertIsNone(animation(POINT, 14, "arrow (standard pointer)"))
        self.assertIsNone(animation(HFX, 108, "spell name #2"))

    def test_page_embeds_entries(self):
        self.assertIn('"name": "x"', page([{"name": "x"}]))
