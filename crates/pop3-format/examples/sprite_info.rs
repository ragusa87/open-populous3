//! `cargo run -p pop3-format --example sprite_info -- path/to/data/POINT0-0.DAT`
use pop3_format::SpriteBank;

fn main() {
    let path = std::env::args().nth(1).expect("usage: sprite_info <psfb file>");
    let bank = SpriteBank::parse(&std::fs::read(&path).expect("read")).expect("parse sprites");
    for (i, s) in bank.sprites.iter().enumerate().filter(|(_, s)| s.pixels.iter().any(Option::is_some)) {
        println!("{i:3}: {}x{} tip {:?}", s.width, s.height, s.tip());
    }
}
