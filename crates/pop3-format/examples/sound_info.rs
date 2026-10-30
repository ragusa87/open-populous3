//! `cargo run -p pop3-format --example sound_info -- path/to/Soundd2.sdt`
//! Prints each entry's index, name, sample rate, channel count, flags, data size and loop range (if any).
//! A music bank (file name containing "drones", case-insensitive - `PopDrones22.SDT`) is read as a
//! `MusicBank` instead: index, name and decoded size.
use pop3_format::sound::{MusicBank, SoundBank};

fn main() {
    let path = std::env::args().nth(1).expect("usage: sound_info <sdt file>");
    let data = std::fs::read(&path).expect("read");
    let is_music = path.to_lowercase().contains("drones");
    if is_music {
        let bank = MusicBank::parse(&data).expect("parse music bank");
        println!("{} entries (music)", bank.entries.len());
        for (i, e) in bank.entries.iter().enumerate() {
            println!("{i:3}: {:10} {:5} Hz  decoded {:9} B  mpeg {:9} B", e.name, e.sample_rate, e.decoded_size, e.mpeg_data(&data).len());
        }
    } else {
        let bank = SoundBank::parse(&data).expect("parse sound bank");
        println!("{} entries", bank.entries.len());
        for (i, e) in bank.entries.iter().enumerate() {
            let channels = if e.is_stereo() { "stereo" } else { "mono" };
            let flags = [e.is_placeholder().then_some("placeholder"), e.is_compressed().then_some("compressed")].into_iter().flatten().collect::<Vec<_>>().join(",");
            let loop_str = e.loop_range.map_or(String::new(), |(s, l)| format!(" loop [{s}, {l})"));
            println!("{i:3}: {:16} {:5} Hz  {channels:6}  {:3} bits  data {:8} B  flags {:#04x}{}{}{}", e.name, e.sample_rate, e.bits, e.data.len(), e.flags, if flags.is_empty() { "" } else { " " }, flags, loop_str);
        }
    }
}
