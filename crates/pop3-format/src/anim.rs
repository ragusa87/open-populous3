//! Person animations: `VSTART-0.ANI`, `VFRA-0.ANI`, `VELE-0.ANI` over the `HSPR0-0.DAT` sprites.
//! See docs/specs/animations.md.

use crate::level::LevelError;
use crate::sprites::{Sprite, SpriteBank};
use std::path::Path;

pub const START_FILE: &str = "vstart-0.ani";
pub const FRAME_FILE: &str = "vfra-0.ani";
pub const ELEMENT_FILE: &str = "vele-0.ani";
/// Person sprites drawn by the elements (palette `pal0-0.dat`).
pub const SPRITE_FILE: &str = "hspr0-0.dat";
/// Directions per animation; 5..8 are 3..1 mirrored.
pub const DIRECTIONS: usize = 8;
/// Longest frame loop followed, guards against broken chains.
const MAX_FRAMES: usize = 64;
/// Element flag: draw the sprite mirrored.
pub const FLAG_FLIP: u16 = 0x0001;
/// Element flag: ground shadow under the person.
pub const FLAG_SHADOW: u16 = 0x0004;
/// Element flag: tribe colour layer, tribe 1-3 in bits 9-10 (the body itself is tribe 0, blue).
pub const FLAG_TRIBE_LAYER: u16 = 0x0010;
/// Element flag: outfit layer, the unit type's gear over the shared tribesman body (see `Outfit`).
pub const FLAG_OUTFIT: u16 = 0x0020;

/// An outfit layer: elements with `FLAG_OUTFIT`, these `0x10 | 0x20` flag bits and this value in
/// bits 9-10 (identified by eye in `catalog`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outfit {
    pub flags: u16,
    pub bits: u16,
}

/// Whether an element is drawn for a person of `tribe` wearing `outfit`: never shadows; outfit
/// layers only for that outfit; tribe layers only for that tribe; everything else always.
pub fn element_shown(flags: u16, tribe: u8, outfit: Option<Outfit>) -> bool {
    let bits = (flags >> 9) & 3;
    if flags & FLAG_SHADOW != 0 {
        false
    } else if flags & FLAG_OUTFIT != 0 {
        outfit == Some(Outfit { flags: flags & (FLAG_TRIBE_LAYER | FLAG_OUTFIT), bits })
    } else if flags & FLAG_TRIBE_LAYER != 0 {
        bits == tribe as u16
    } else {
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Start {
    pub frame: u16,
    /// Drawn as the mirror image of the given direction's frames.
    pub mirrored: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub first_element: u16,
    pub next: u16,
}

/// One sprite placed relative to the person's feet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Element {
    /// Index in the `HSPR0-0.DAT` bank, None for an empty element.
    pub sprite: Option<usize>,
    pub x: i16,
    pub y: i16,
    pub flags: u16,
    pub next: u16,
}

#[derive(Clone, Debug, Default)]
pub struct AnimBank {
    pub starts: Vec<[Start; DIRECTIONS]>,
    pub frames: Vec<Frame>,
    pub elements: Vec<Element>,
}

/// A composed frame: palette indices, with the feet at `origin`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub sprite: Sprite,
    pub origin: (usize, usize),
}

fn u16_at(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}

impl AnimBank {
    /// `VSTART`: per animation, 8 x (u16 frame, u16 mirror source, 0 = own frames).
    /// `VFRA`: 8 bytes (u16 first element, u8 w, u8 h, u16 flags, u16 next frame).
    /// `VELE`: 10 bytes (u16 (sprite + 1) x 6, i16 x, i16 y, u16 flags, u16 next element).
    pub fn parse(start: &[u8], frames: &[u8], elements: &[u8]) -> Result<Self, LevelError> {
        let starts = start
            .chunks_exact(4 * DIRECTIONS)
            .map(|a| std::array::from_fn(|d| Start { frame: u16_at(a, d * 4), mirrored: u16_at(a, d * 4 + 2) != 0 }))
            .collect();
        let frames = frames.chunks_exact(8).map(|f| Frame { first_element: u16_at(f, 0), next: u16_at(f, 6) }).collect();
        let elements = elements
            .chunks_exact(10)
            .map(|e| Element {
                sprite: (u16_at(e, 0) as usize / 6).checked_sub(1),
                x: u16_at(e, 2) as i16,
                y: u16_at(e, 4) as i16,
                flags: u16_at(e, 6),
                next: u16_at(e, 8),
            })
            .collect();
        let bank = AnimBank { starts, frames, elements };
        if bank.starts.is_empty() || bank.frames.is_empty() {
            return Err(LevelError::BadSize { expected: 4 * DIRECTIONS, got: start.len() });
        }
        Ok(bank)
    }

    pub fn load(data_dir: &Path) -> Result<Self, LevelError> {
        let read = |n: &str| std::fs::read(crate::find_file(data_dir, n).unwrap_or_else(|| data_dir.join(n)));
        Self::parse(&read(START_FILE)?, &read(FRAME_FILE)?, &read(ELEMENT_FILE)?)
    }

    pub fn start(&self, anim: usize, dir: usize) -> Option<Start> {
        self.starts.get(anim).map(|s| s[dir % DIRECTIONS])
    }

    /// Frame indices of one loop, following `next` until it comes back (or breaks).
    pub fn frame_loop(&self, anim: usize, dir: usize) -> Vec<u16> {
        let Some(first) = self.start(anim, dir).map(|s| s.frame).filter(|&f| f != 0) else { return Vec::new() };
        let mut out = vec![first];
        let mut f = first;
        while let Some(next) = self.frames.get(f as usize).map(|fr| fr.next) {
            if next == 0 || next == first || out.len() >= MAX_FRAMES {
                break;
            }
            out.push(next);
            f = next;
        }
        out
    }

    /// Elements drawn for a frame, back to front.
    pub fn frame_elements(&self, frame: u16) -> Vec<Element> {
        let mut out = Vec::new();
        let mut e = self.frames.get(frame as usize).map_or(0, |f| f.first_element);
        while let Some(&el) = self.elements.get(e as usize).filter(|_| e != 0 && out.len() < MAX_FRAMES) {
            out.push(el);
            e = el.next;
        }
        out
    }

    /// The person itself: elements without shadow or tribe layers, mirrored if asked.
    pub fn compose(&self, sprites: &SpriteBank, frame: u16, mirrored: bool) -> Picture {
        self.compose_as(sprites, frame, mirrored, 0, None)
    }

    /// The person as a unit of `tribe` wearing `outfit` (see `element_shown`), mirrored if asked.
    pub fn compose_as(&self, sprites: &SpriteBank, frame: u16, mirrored: bool, tribe: u8, outfit: Option<Outfit>) -> Picture {
        let parts: Vec<(Element, &Sprite)> = self
            .frame_elements(frame)
            .into_iter()
            .filter(|e| element_shown(e.flags, tribe, outfit))
            .filter_map(|e| Some((e, sprites.sprites.get(e.sprite?)?)))
            .collect();
        compose_parts(&parts, mirrored)
    }
}

fn compose_parts(parts: &[(Element, &Sprite)], mirrored: bool) -> Picture {
    let (mut x0, mut y0, mut x1, mut y1) = (0i32, 0i32, 0i32, 0i32);
    for (e, s) in parts {
        x0 = x0.min(e.x as i32);
        y0 = y0.min(e.y as i32);
        x1 = x1.max(e.x as i32 + s.width as i32);
        y1 = y1.max(e.y as i32 + s.height as i32);
    }
    let (w, h) = ((x1 - x0).max(1) as usize, (y1 - y0).max(1) as usize);
    let mut pixels = vec![None; w * h];
    for (e, s) in parts {
        let flip = e.flags & FLAG_FLIP != 0;
        for sy in 0..s.height {
            for sx in 0..s.width {
                let Some(c) = s.pixels[sy * s.width + if flip { s.width - 1 - sx } else { sx }] else { continue };
                let (px, py) = ((e.x as i32 + sx as i32 - x0) as usize, (e.y as i32 + sy as i32 - y0) as usize);
                pixels[py * w + px] = Some(c);
            }
        }
    }
    let mut origin = ((-x0) as usize, (-y0) as usize);
    if mirrored {
        for row in pixels.chunks_exact_mut(w) {
            row.reverse();
        }
        origin.0 = w - origin.0;
    }
    Picture { sprite: Sprite { width: w, height: h, pixels }, origin }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layers_shown_per_tribe_and_outfit() {
        let fire = Outfit { flags: 0x20, bits: 1 };
        let monk = Outfit { flags: 0x30, bits: 1 };
        assert!(element_shown(0x0000, 2, None), "the body");
        assert!(!element_shown(0x0204, 1, None), "shadow");
        assert!(element_shown(0x0410, 2, None) && !element_shown(0x0410, 1, None) && !element_shown(0x0410, 0, None), "yellow loincloth for tribe 2 only");
        assert!(element_shown(0x0220, 0, Some(fire)) && !element_shown(0x0220, 0, Some(monk)) && !element_shown(0x0220, 0, None));
        assert!(element_shown(0x0230, 3, Some(monk)) && !element_shown(0x0230, 1, Some(fire)), "monk helmet whatever the tribe");
    }

    fn le(v: &[u16]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    /// Anim 0: dir 0 loops frames 1 -> 2 -> 1, dir 5 mirrors dir 3.
    fn bank() -> AnimBank {
        let mut start = vec![0u16; 2 * DIRECTIONS];
        start[0] = 1;
        start[10] = 2;
        start[11] = 3;
        let frames = le(&[0, 0, 0, 0, 1, 0x1515, 0, 2, 3, 0x1515, 0, 1]);
        let elements = le(&[0, 0, 0, 0, 0, 12, (-1i16) as u16, (-2i16) as u16, 0, 2, 18, 0, 0, FLAG_SHADOW, 0, 12, 0, 0, FLAG_FLIP, 0]);
        AnimBank::parse(&le(&start), &frames, &elements).unwrap()
    }

    fn sprites() -> SpriteBank {
        let s = |w, h, p: Vec<Option<u8>>| Sprite { width: w, height: h, pixels: p };
        SpriteBank { sprites: vec![s(1, 1, vec![None]), s(2, 1, vec![Some(1), Some(2)]), s(1, 1, vec![Some(9)])] }
    }

    #[test]
    fn parses_starts_and_mirrors() {
        let b = bank();
        assert_eq!(b.start(0, 0), Some(Start { frame: 1, mirrored: false }));
        assert_eq!(b.start(0, 5), Some(Start { frame: 2, mirrored: true }));
        assert_eq!(b.elements[1].sprite, Some(1), "sprite field is (index + 1) x 6");
        assert_eq!(b.elements[0].sprite, None);
        assert_eq!((b.elements[1].x, b.elements[1].y), (-1, -2));
    }

    #[test]
    fn frame_loop_stops_where_it_started() {
        let b = bank();
        assert_eq!(b.frame_loop(0, 0), vec![1, 2]);
        assert_eq!(b.frame_loop(0, 1), Vec::<u16>::new(), "no frames");
        assert_eq!(b.frame_loop(9, 0), Vec::<u16>::new(), "no such anim");
    }

    #[test]
    fn element_chain_and_compose_skip_the_shadow() {
        let b = bank();
        assert_eq!(b.frame_elements(1).len(), 2);
        let p = b.compose(&sprites(), 1, false);
        assert_eq!((p.sprite.width, p.sprite.height, p.origin), (2, 2, (1, 2)));
        assert_eq!(p.sprite.pixels, vec![Some(1), Some(2), None, None]);
    }

    #[test]
    fn element_flip_and_frame_mirror() {
        let b = bank();
        let p = b.compose(&sprites(), 2, false);
        assert_eq!((p.sprite.pixels.clone(), p.origin), (vec![Some(2), Some(1)], (0, 0)));
        let m = b.compose(&sprites(), 2, true);
        assert_eq!((m.sprite.pixels, m.origin), (vec![Some(1), Some(2)], (2, 0)));
    }
}
