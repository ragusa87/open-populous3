//! Index of a baked unit atlas (docs/specs/unit-art.md, "Baked atlas"): where each frame of a kind's
//! poses sits in its atlas PNG, and where its feet are. A text file, one frame per line after the
//! header: `pose dir frame x y width height feet_x feet_y`, frames of a pose and direction in play order.
//! Lines starting with `#` are comments.

pub const HEADER: &str = "unit-atlas 1";

const COMMENT: &str = "\
# Baked by `just bake-units` (crates/unit-baker), see docs/specs/unit-art.md.
# pose dir frame x y width height feet_x feet_y
# x y width height: the frame's rectangle in the atlas PNG; feet_x feet_y: from its top-left corner.
";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub pose: String,
    pub dir: usize,
    pub frame: usize,
    /// Top-left corner and size in the atlas, in pixels.
    pub rect: [u32; 4],
    /// The feet, in pixels from the rectangle's top-left corner.
    pub origin: (u32, u32),
}

pub fn write(entries: &[Entry]) -> String {
    let mut out = format!("{HEADER}\n{COMMENT}");
    for e in entries {
        let [x, y, w, h] = e.rect;
        out += &format!("{} {} {} {x} {y} {w} {h} {} {}\n", e.pose, e.dir, e.frame, e.origin.0, e.origin.1);
    }
    out
}

pub fn parse(text: &str) -> Result<Vec<Entry>, String> {
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    if lines.next().map(|(_, l)| l.trim()) != Some(HEADER) {
        return Err(format!("not a `{HEADER}` index"));
    }
    lines
        .map(|(i, line)| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let bad = || format!("line {}: `{line}`", i + 1);
            let [pose, rest @ ..] = fields.as_slice() else { return Err(bad()) };
            let n: Vec<u32> = rest.iter().map(|v| v.parse()).collect::<Result<_, _>>().map_err(|_| bad())?;
            let [dir, frame, x, y, w, h, ox, oy] = n.as_slice() else { return Err(bad()) };
            Ok(Entry { pose: pose.to_string(), dir: *dir as usize, frame: *frame as usize, rect: [*x, *y, *w, *h], origin: (*ox, *oy) })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let entries = vec![
            Entry { pose: "idle".into(), dir: 0, frame: 0, rect: [0, 0, 52, 130], origin: (26, 122) },
            Entry { pose: "walk".into(), dir: 7, frame: 11, rect: [1000, 260, 60, 128], origin: (30, 120) },
        ];
        let text = write(&entries);
        assert_eq!(text.lines().next(), Some(HEADER));
        assert!(text.lines().nth(1).unwrap().starts_with('#'));
        assert_eq!(text.lines().find(|l| !l.starts_with('#') && *l != HEADER), Some("idle 0 0 0 0 52 130 26 122"));
        assert_eq!(parse(&text), Ok(entries));
    }

    #[test]
    fn rejects_malformed_lines() {
        assert!(parse("idle 0 0 0 0 1 1 0 0\n").is_err(), "no header");
        assert!(parse(&format!("{HEADER}\nidle 0 0 0 0 1 1 0\n")).is_err(), "a field short");
        assert!(parse(&format!("{HEADER}\nidle 0 0 0 0 1 1 0 x\n")).is_err());
        assert_eq!(parse(&format!("{HEADER}\n")), Ok(vec![]));
        assert_eq!(parse(&format!("{HEADER}\n#\nidle 0 0 0 0 1 1 x 0\n")), Err("line 3: `idle 0 0 0 0 1 1 x 0`".into()));
    }

    #[test]
    fn skips_comments() {
        let entry = Entry { pose: "idle".into(), dir: 0, frame: 0, rect: [0, 0, 1, 1], origin: (0, 0) };
        let text = format!("# before\n{HEADER}\n# pose dir ...\n  # indented\n\nidle 0 0 0 0 1 1 0 0\n# after\n");
        assert_eq!(parse(&text), Ok(vec![entry]));
    }
}
