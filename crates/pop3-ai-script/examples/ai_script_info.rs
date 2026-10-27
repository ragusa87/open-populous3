//! `cargo run -p pop3-ai-script --example ai_script_info -- path/to/cpscrNNN.dat`
//! Parses a real file from the user's own install (never committed) and dumps the statement tree,
//! as a manual sanity check against real data.
use pop3_ai_script::{ast, RawScript};

fn main() {
    let path = std::env::args().nth(1).expect("usage: ai_script_info <cpscrNNN.dat>");
    let data = std::fs::read(&path).expect("read file");
    let raw = RawScript::parse(&data).expect("parse raw layout");
    let script = ast::parse(&raw).expect("parse script");
    println!("version {}, fields {}", script.version, raw.fields.len());
    println!("top-level statements: {}", script.block.0.len());
    println!("{:#?}", script.block);
}
