import struct
import unittest

from sprite_catalog import (CATALOG, FLAG_FLIP, FLAG_SHADOW, HFX, POINT, alpha_colours, animation, anim_name,
                             compose_frame, element_shown, frame_elements, frame_loop, is_alpha, page, parse_elements,
                             parse_frames, parse_starts)


def le(values):
    """Little-endian u16 values packed as bytes, mirroring `crates/pop3-format/src/anim.rs` tests' `le`."""
    return b"".join(struct.pack("<H", v & 0xFFFF) for v in values)


def anim_fixture():
    """One animation: dir 0 loops frames 1 -> 2 -> 1, dir 5 mirrors dir 3 (`crates/pop3-format/src/anim.rs`'s
    `bank()`, ported byte for byte so both parsers agree)."""
    start = [0] * 16
    start[0], start[10], start[11] = 1, 2, 3
    frames = le([0, 0, 0, 0, 1, 0x1515, 0, 2, 3, 0x1515, 0, 1])
    elements = le([0, 0, 0, 0, 0, 12, -1, -2, 0, 2, 18, 0, 0, FLAG_SHADOW, 0, 12, 0, 0, FLAG_FLIP, 0])
    return parse_starts(le(start)), parse_frames(frames), parse_elements(elements)


FIXTURE_SPRITES = [(1, 1, [[None]]), (2, 1, [[1, 2]]), (1, 1, [[9]])]


class AnimTest(unittest.TestCase):
    def test_parses_starts_and_mirrors(self):
        starts, _, elements = anim_fixture()
        self.assertEqual(starts[0][0], (1, 0))
        self.assertEqual(starts[0][5], (2, 3))
        self.assertEqual(elements[1][0], 1, "sprite field is (index + 1) x 6")
        self.assertIsNone(elements[0][0])
        self.assertEqual((elements[1][1], elements[1][2]), (-1, -2))

    def test_frame_loop_stops_where_it_started(self):
        starts, frames, _ = anim_fixture()
        self.assertEqual(frame_loop(starts, frames, 0, 0), [1, 2])
        self.assertEqual(frame_loop(starts, frames, 0, 1), [], "no frames")
        self.assertEqual(frame_loop(starts, frames, 9, 0), [], "no such anim")

    def test_element_chain_and_compose_skip_the_shadow(self):
        starts, frames, elements = anim_fixture()
        self.assertEqual(len(frame_elements(frames, elements, 1)), 2, "the shadow is chained but not drawn")
        w, h, pixels = compose_frame(starts, frames, elements, FIXTURE_SPRITES, 0, 0, 0, None)[0]
        self.assertEqual((w, h), (2, 2))
        self.assertEqual(pixels, [[1, 2], [None, None]])

    def test_element_flip_and_frame_mirror(self):
        starts, frames, elements = anim_fixture()
        # Direction 0's loop is [1, 2]: frame 2's lone element is drawn flipped (`FLAG_FLIP`).
        composed = compose_frame(starts, frames, elements, FIXTURE_SPRITES, 0, 0, 0, None)
        self.assertEqual(composed[1], (2, 1, [[2, 1]]))
        # Direction 5 mirrors direction 3 and starts straight at frame 2 (`start[5] == (2, mirrored)`).
        mirrored = compose_frame(starts, frames, elements, FIXTURE_SPRITES, 0, 5, 0, None)
        self.assertEqual(mirrored[0], (2, 1, [[1, 2]]), "frame-mirrored on top of the element's own flip")

    def test_element_shown_by_tribe_and_outfit(self):
        fire, monk = (0x20, 1), (0x30, 1)
        self.assertTrue(element_shown(0x0000, 2, None), "the body")
        self.assertFalse(element_shown(0x0204, 1, None), "shadow")
        self.assertTrue(element_shown(0x0410, 2, None) and not element_shown(0x0410, 1, None), "tribe layer")
        self.assertTrue(element_shown(0x0220, 0, fire) and not element_shown(0x0220, 0, monk) and not element_shown(0x0220, 0, None))
        self.assertTrue(element_shown(0x0230, 3, monk) and not element_shown(0x0230, 1, fire), "monk helmet whatever the tribe")

    def test_anim_name_known_and_shaman_tribes(self):
        self.assertEqual(anim_name(6), "idle, standing breathing")
        self.assertEqual(anim_name(53), "shaman: idle")
        self.assertEqual(anim_name(56), "shaman: idle (tribe 3)")
        self.assertIsNone(anim_name(82), "not identified (gap between the old and real shaman bodies)")


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
