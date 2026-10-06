//! `cargo run -p pop3-format --example objects_info -- path/to/objects [bank]`
use pop3_format::ObjectBank;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: objects_info <objects dir> [bank]");
    let bank: u8 = std::env::args().nth(2).and_then(|b| b.parse().ok()).unwrap_or(0);
    let objs = ObjectBank::load(dir.as_ref(), bank).expect("parse objects");
    println!("bank {bank}: {} slots", objs.objects.len());
    for (i, o) in objs.objects.iter().enumerate().filter(|(_, o)| !o.is_empty()) {
        let min = (0..3).map(|k| o.points.iter().map(|p| p[k]).min().unwrap_or(0)).collect::<Vec<_>>();
        let max = (0..3).map(|k| o.points.iter().map(|p| p[k]).max().unwrap_or(0)).collect::<Vec<_>>();
        println!("  {i:3}: {:3} faces {:3} points, min {min:?} max {max:?}", o.faces.len(), o.points.len());
    }
}
