//! `cargo run -p pop3-format --example level_info -- path/to/levl2001.dat`
use pop3_format::{Level, LevelHeader};

fn main() {
    let path = std::env::args().nth(1).expect("usage: level_info <levlXXXX.dat>");
    let lvl = Level::load(&path).expect("parse level");
    let name = LevelHeader::load(path.replace(".dat", ".hdr")).map(|h| h.name).unwrap_or_default();
    let max = lvl.heights.iter().max().unwrap();
    let land = lvl.heights.iter().filter(|&&h| h > 0).count();
    println!("{name}: max height {max}, land cells {land}, things {}", lvl.things.len());
    for t in lvl.things.iter().take(10) {
        println!("  model {:3} kind {} owner {:3} at ({:5},{:5})", t.model, t.kind, t.owner, t.x, t.z);
    }
}
