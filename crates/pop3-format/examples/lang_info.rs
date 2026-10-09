//! `cargo run -p pop3-format --example lang_info -- path/to/language [file number] [first] [count]`
//! Prints the numbered texts of `langNN.dat` (default 0, English), one per line, line breaks shown as `\n`.
use pop3_format::language::{Language, LANGUAGES};

fn main() {
    let dir = std::env::args().nth(1).expect("usage: lang_info <language dir> [file number] [first] [count]");
    let arg = |i: usize, default: usize| std::env::args().nth(i).and_then(|v| v.parse().ok()).unwrap_or(default);
    let number = arg(2, 0) as u8;
    let lang = Language::load(dir.as_ref(), number).expect("read language file");
    let name = LANGUAGES.iter().find(|l| l.0 == number).map_or("placeholder", |l| l.1);
    println!("lang{number:02}.dat ({name}): {} texts", lang.texts.len());
    for (n, text) in lang.texts.iter().enumerate().skip(arg(3, 0)).take(arg(4, usize::MAX)) {
        println!("{n:5} {}", text.replace('\n', "\\n").replace('\r', "\\r"));
    }
}
