//! Generated shaman sprites (no original files): a small pixel-art figure in the tribe colour,
//! posed per frame and drawn from a few thick lines and discs with a dark outline.
//! Figure space: x to the right, y up, feet at (0, 0), about 34 px tall like the original.

use super::art::{Frame, Pose};
use std::f32::consts::{FRAC_PI_2, PI, TAU};

const W: usize = 72;
const H: usize = 64;
const ORIGIN: (usize, usize) = (36, 58);

const OUTLINE: [u8; 3] = [30, 18, 10];
const SKIN: [u8; 3] = [214, 150, 100];
const HAIR: [u8; 3] = [40, 26, 18];
const FEATHER: [u8; 3] = [236, 222, 180];
const STAFF: [u8; 3] = [120, 78, 36];
const SPARK: [u8; 3] = [240, 120, 255];
const WATER: [u8; 3] = [170, 210, 255];

#[derive(Clone, Copy, PartialEq, Debug)]
enum View {
    Front,
    Side,
    Back,
}

/// Drawn view per direction (see `art::sprite_dir`), mirrored for the left-hand ones.
fn view(dir: usize) -> (View, bool) {
    match dir % 8 {
        0 | 1 => (View::Front, false),
        7 => (View::Front, true),
        2 => (View::Side, false),
        6 => (View::Side, true),
        3 | 4 => (View::Back, false),
        _ => (View::Back, true),
    }
}

/// Original tribe colours: blue, red, yellow, green.
pub fn tribe_rgb(tribe: u8) -> [u8; 3] {
    match tribe {
        0 => [38, 89, 242],
        1 => [230, 38, 25],
        2 => [242, 217, 38],
        3 => [51, 191, 51],
        _ => [150, 150, 150],
    }
}

pub fn frame_count(pose: Pose) -> usize {
    match pose {
        Pose::Idle | Pose::Pray | Pose::Drown => 4,
        Pose::Walk | Pose::Fall => 8,
        Pose::Cast => 12,
    }
}

/// Joint positions for one frame, in figure space before tilt/lift.
#[derive(Clone, Copy, Debug)]
struct Body {
    feet: [(f32, f32); 2],
    hip: f32,
    shoulder: f32,
    head: (f32, f32),
    hands: [(f32, f32); 2],
    /// Whole figure raised (jump) or lowered (sinking), in pixels.
    lift: f32,
    /// Rotation about the feet (falling), radians, positive = clockwise (towards +x).
    tilt: f32,
    sparks: bool,
}

fn body(pose: Pose, view: View, f: usize) -> Body {
    let n = frame_count(pose);
    let phase = f as f32 / n as f32 * TAU;
    let side = view == View::Side;
    let base = Body {
        feet: [(-2.0, 0.0), (2.0, 0.0)],
        hip: 12.0,
        shoulder: 21.0,
        head: (0.0, 25.5),
        hands: if side { [(2.0, 12.0), (2.0, 12.0)] } else { [(-6.0, 13.0), (6.0, 13.0)] },
        lift: 0.0,
        tilt: 0.0,
        sparks: false,
    };
    match pose {
        Pose::Idle => {
            let breath = (f % 2) as f32 * 0.5;
            Body { shoulder: 21.0 + breath, head: (0.0, 25.5 + breath), ..base }
        }
        Pose::Walk => {
            let s = phase.sin();
            let feet = if side {
                [(-4.0 * s, 0.0), (4.0 * s, 0.0)]
            } else {
                [(-2.0, (s * 3.0).max(0.0)), (2.0, (-s * 3.0).max(0.0))]
            };
            let hands = if side { [(3.0 * s, 12.0), (-3.0 * s, 12.0)] } else { base.hands };
            Body { feet, hands, lift: s.abs(), ..base }
        }
        Pose::Pray => {
            let bow = [0.0, 1.0, 2.0, 1.0][f % 4];
            Body {
                feet: if side { [(-5.0, 0.0), (-4.0, 0.0)] } else { [(-5.0, 0.0), (5.0, 0.0)] },
                hip: 4.0,
                shoulder: 13.0 - bow,
                head: (if side { bow } else { 0.0 }, 17.0 - bow),
                hands: if side { [(7.0, 14.0 - bow), (7.0, 14.0 - bow)] } else { [(-10.0, 15.0 - bow), (10.0, 15.0 - bow)] },
                ..base
            }
        }
        Pose::Cast => {
            let t = f as f32 / (n - 1) as f32;
            let up = (t * PI).sin();
            Body {
                hands: if side { [(5.0, 28.0), (6.0, 30.0)] } else { [(-7.0, 30.0), (7.0, 30.0)] },
                feet: [(-1.5, 2.0 * up), (1.5, 2.0 * up)],
                lift: 12.0 * up,
                sparks: f > 1 && f < n - 1,
                ..base
            }
        }
        Pose::Fall => {
            let t = (f as f32 / (n - 2) as f32).min(1.0);
            Body { tilt: t * FRAC_PI_2, hands: [(-8.0, 20.0), (8.0, 20.0)], ..base }
        }
        Pose::Drown => {
            let k = (f % 2) as f32 * 3.0;
            Body { hands: [(-8.0, 26.0 + k), (8.0, 29.0 - k)], lift: -8.0 - (f % 2) as f32, ..base }
        }
    }
}

enum Shape {
    Line((f32, f32), (f32, f32), f32, [u8; 3]),
    Disc((f32, f32), f32, [u8; 3]),
    Dot((f32, f32), [u8; 3]),
}

fn shapes(b: &Body, view: View, tribe: u8) -> Vec<Shape> {
    let robe = tribe_rgb(tribe);
    let dark = robe.map(|c| (c as u16 * 3 / 5) as u8);
    let side = view == View::Side;
    let mut s = Vec::new();
    let staff_hand = b.hands[1];
    let staff = |s: &mut Vec<Shape>| {
        s.push(Shape::Line((staff_hand.0, staff_hand.1 - 11.0), (staff_hand.0, staff_hand.1 + 15.0), 1.0, STAFF));
        s.push(Shape::Disc((staff_hand.0, staff_hand.1 + 16.0), 1.5, robe));
    };
    let headdress = |s: &mut Vec<Shape>| {
        for k in 0..7 {
            let a = PI * (0.15 + 0.7 * k as f32 / 6.0);
            let tip = (b.head.0 + a.cos() * 8.0, b.head.1 + a.sin() * 8.0);
            s.push(Shape::Line(b.head, tip, 1.5, if k % 2 == 0 { FEATHER } else { robe }));
        }
    };
    if view == View::Back {
        headdress(&mut s);
    }
    if !side {
        staff(&mut s);
    }
    for foot in b.feet {
        let kneeling = if b.hip < 8.0 { foot.0.signum() * 2.0 } else { 0.0 };
        let knee = (foot.0 + kneeling, (b.hip + foot.1) / 2.0 + 1.0);
        s.push(Shape::Line((foot.0.signum() * 1.5, b.hip), knee, 2.0, SKIN));
        s.push(Shape::Line(knee, foot, 2.0, SKIN));
    }
    s.push(Shape::Line((0.0, b.hip), (0.0, b.shoulder), if side { 5.0 } else { 7.0 }, robe));
    s.push(Shape::Line((0.0, b.hip + 1.0), (0.0, b.hip - 1.0), if side { 6.0 } else { 8.0 }, dark));
    let shoulders: [(f32, f32); 2] = if side { [(0.5, b.shoulder), (0.5, b.shoulder)] } else { [(-3.5, b.shoulder), (3.5, b.shoulder)] };
    for (sh, hand) in shoulders.iter().zip(b.hands) {
        s.push(Shape::Line(*sh, hand, 2.0, SKIN));
    }
    if side {
        staff(&mut s);
    }
    s.push(Shape::Disc(b.head, 3.2, if view == View::Back { HAIR } else { SKIN }));
    match view {
        View::Front => {
            s.push(Shape::Dot((b.head.0 - 1.0, b.head.1 + 0.5), HAIR));
            s.push(Shape::Dot((b.head.0 + 1.0, b.head.1 + 0.5), HAIR));
            headdress(&mut s);
        }
        View::Side => {
            s.push(Shape::Dot((b.head.0 + 3.5, b.head.1), SKIN));
            s.push(Shape::Dot((b.head.0 + 1.5, b.head.1 + 0.5), HAIR));
            for k in 0..4 {
                let a = PI * (0.5 + 0.12 * k as f32);
                let tip = (b.head.0 - 1.0 + a.cos() * 8.0, b.head.1 + a.sin() * 8.0);
                s.push(Shape::Line((b.head.0 - 1.0, b.head.1), tip, 1.5, if k % 2 == 0 { FEATHER } else { robe }));
            }
        }
        View::Back => {}
    }
    if b.sparks {
        for (i, hand) in b.hands.iter().enumerate() {
            for k in 0..5 {
                let a = (k as f32 * 1.3 + i as f32 * 2.0 + b.lift) % TAU;
                s.push(Shape::Dot((hand.0 + a.cos() * 3.0, hand.1 + 1.0 + a.sin() * 3.0), SPARK));
            }
        }
    }
    s
}

struct Canvas {
    rgba: Vec<u8>,
}

impl Canvas {
    fn plot(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if (0..W as i32).contains(&x) && (0..H as i32).contains(&y) {
            let i = (y as usize * W + x as usize) * 4;
            self.rgba[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

/// Figure space to canvas pixel, after the body's tilt and lift; None below the water line when sinking.
fn to_pixel(b: &Body, (x, y): (f32, f32)) -> Option<(i32, i32)> {
    let (sin, cos) = b.tilt.sin_cos();
    let (rx, ry) = (x * cos + y * sin, -x * sin + y * cos + b.lift);
    if b.lift < 0.0 && ry < 0.0 {
        return None;
    }
    Some(((ORIGIN.0 as f32 + rx).floor() as i32, (ORIGIN.1 as f32 - ry).ceil() as i32 - 1))
}

fn rasterise(c: &mut Canvas, b: &Body, shape: &Shape, grow: f32, colour: Option<[u8; 3]>) {
    let mut fill = |centre: (f32, f32), r: f32, col: [u8; 3]| {
        let r = r + grow;
        let steps = r.ceil() as i32;
        for dy in -steps..=steps {
            for dx in -steps..=steps {
                let p = (centre.0 + dx as f32, centre.1 + dy as f32);
                if (dx * dx + dy * dy) as f32 <= r * r + 0.5 {
                    if let Some((px, py)) = to_pixel(b, p) {
                        c.plot(px, py, colour.unwrap_or(col));
                    }
                }
            }
        }
    };
    match *shape {
        Shape::Line(a, e, th, col) => {
            let len = ((e.0 - a.0).powi(2) + (e.1 - a.1).powi(2)).sqrt();
            let n = (len * 2.0).ceil().max(1.0) as i32;
            for i in 0..=n {
                let t = i as f32 / n as f32;
                fill((a.0 + (e.0 - a.0) * t, a.1 + (e.1 - a.1) * t), (th - 1.0) / 2.0, col);
            }
        }
        Shape::Disc(p, r, col) => fill(p, r - 0.5, col),
        Shape::Dot(p, col) => fill(p, 0.0, col),
    }
}

/// Bounding box of the opaque pixels (keeping the feet inside), origin moved accordingly.
fn cropped(rgba: &[u8], mirrored: bool) -> Frame {
    let opaque = |x: usize, y: usize| rgba[(y * W + x) * 4 + 3] != 0;
    let (mut x0, mut y0, mut x1, mut y1) = (ORIGIN.0, ORIGIN.1, ORIGIN.0, ORIGIN.1);
    for y in 0..H {
        for x in 0..W {
            if opaque(x, y) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    let (w, h) = (x1 - x0, y1 - y0);
    let mut out = Vec::with_capacity(w * h * 4);
    for y in y0..y1 {
        let row = &rgba[(y * W + x0) * 4..(y * W + x1) * 4];
        if mirrored {
            out.extend(row.chunks_exact(4).rev().flatten());
        } else {
            out.extend_from_slice(row);
        }
    }
    let ox = ORIGIN.0 - x0;
    Frame { width: w, height: h, origin: (if mirrored { w - ox } else { ox }, ORIGIN.1 - y0), rgba: out }
}

/// The frame loop of a pose seen from `dir`, in the tribe's colour.
pub fn frames(tribe: u8, pose: Pose, dir: usize) -> Vec<Frame> {
    let (view, mirrored) = view(dir);
    (0..frame_count(pose))
        .map(|f| {
            let b = body(pose, view, f);
            let list = shapes(&b, view, tribe);
            let mut c = Canvas { rgba: vec![0; W * H * 4] };
            for s in &list {
                rasterise(&mut c, &b, s, 1.0, Some(OUTLINE));
            }
            for s in &list {
                rasterise(&mut c, &b, s, 0.0, None);
            }
            if pose == Pose::Drown {
                for x in -9..=9 {
                    c.plot(ORIGIN.0 as i32 + x, ORIGIN.1 as i32 - (x + f as i32).rem_euclid(3).min(1), WATER);
                }
            }
            cropped(&c.rgba, mirrored)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_rows(f: &Frame) -> usize {
        (0..f.height).filter(|y| (0..f.width).any(|x| f.rgba[(y * f.width + x) * 4 + 3] != 0)).count()
    }

    #[test]
    fn standing_figure_is_about_original_height() {
        let f = &frames(0, Pose::Idle, 0)[0];
        assert!((30..=40).contains(&f.origin.1), "feet at the bottom, {} px tall", f.origin.1);
        assert_eq!(f.height, f.origin.1 + 1, "outline row under the feet");
        assert!(opaque_rows(f) > 30);
    }

    #[test]
    fn cast_jumps_and_fall_ends_lying() {
        let cast = frames(1, Pose::Cast, 0);
        let top = cast[5].origin.1 as i32 - cast[0].origin.1 as i32;
        assert!(top > 8, "mid-jump is higher: {top}");
        let fall = frames(1, Pose::Fall, 0);
        let lying = &fall[6];
        assert!(lying.width > lying.height, "{}x{}", lying.width, lying.height);
    }

    #[test]
    fn left_views_mirror_right_views() {
        let (r, l) = (&frames(2, Pose::Walk, 2)[3], &frames(2, Pose::Walk, 6)[3]);
        assert_eq!((r.width, r.height), (l.width, l.height));
        assert_eq!(r.origin.0, l.width - l.origin.0);
        let row = |f: &Frame, y: usize| f.rgba[y * f.width * 4..(y + 1) * f.width * 4].to_vec();
        let flipped: Vec<u8> = row(r, 10).chunks_exact(4).rev().flatten().copied().collect();
        assert_eq!(row(l, 10), flipped);
    }

    /// Pixels between the feet and the lowest robe pixel.
    fn robe_height(f: &Frame) -> usize {
        let robe = tribe_rgb(0);
        let lowest = (0..f.height).rev().find(|y| (0..f.width).any(|x| f.rgba[(y * f.width + x) * 4..][..3] == robe));
        f.origin.1 - lowest.unwrap()
    }

    #[test]
    fn drowning_sinks_below_the_water_line() {
        let idle = robe_height(&frames(0, Pose::Idle, 0)[0]);
        let drown = robe_height(&frames(0, Pose::Drown, 0)[0]);
        assert!(drown + 6 < idle, "robe {drown} px above water vs {idle} standing");
    }
}
