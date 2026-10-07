//! `cargo run -p pop3-format --example level_info -- path/to/levl2001.dat [all]`
//! Prints the header, version, map blocks and the first things (all of them with `all`).
use pop3_format::header::{cell_of, LevelFlags};
use pop3_format::{Level, LevelHeader, LevelVersion, ThingData};

fn main() {
    let path = std::env::args().nth(1).expect("usage: level_info <levlXXXX.dat> [all]");
    let all = std::env::args().nth(2).is_some_and(|a| a == "all");
    let lvl = Level::load(&path).expect("parse level");
    let hdr = LevelHeader::load(path.replace(".dat", ".hdr")).unwrap_or_default();
    let max = lvl.heights.iter().max().unwrap();
    let land = lvl.heights.iter().filter(|&&h| h > 0).count();
    println!("{}: max height {max}, land cells {land}, things {}", hdr.name, lvl.things.len());
    if let Ok(ver) = LevelVersion::load(path.replace(".dat", ".ver")) {
        println!("  version {} by {:?} on {:?}, checksum {}", ver.version, ver.created_by, ver.created_on, ver.checksum);
    }
    println!("  theme {}, object bank {}, tribes {}, flags {:#04x}", hdr.theme, hdr.object_bank, hdr.tribes, hdr.flags.0);
    for (flag, name) in [(LevelFlags::FOG_OF_WAR, "fog of war"), (LevelFlags::SHAMAN_OMNI, "shaman omni"), (LevelFlags::NO_GUEST_SPELLS, "no guest spells"), (LevelFlags::NO_REINCARNATION_TIME, "no reincarnation time")] {
        if hdr.flags.has(flag) {
            println!("    {name}");
        }
    }
    let models = |n: u8, f: &dyn Fn(u8) -> bool| (1..n).filter(|&m| f(m)).map(|m| m.to_string()).collect::<Vec<_>>().join(",");
    println!("  spells [{}] buildings [{}] vehicles [{}]", models(22, &|m| hdr.spell_available(m)), models(20, &|m| hdr.building_available(m)), models(5, &|m| hdr.vehicle_available(m)));
    println!("  AI scripts (blue, red, yellow, green) {:?}, allies {:x?}", hdr.ai_scripts, hdr.allies);
    println!("  start cell {:?} angle {}", hdr.start_cell(), hdr.start_angle);
    let markers: Vec<_> = hdr.markers.iter().enumerate().filter(|&(_, &m)| m != 0).map(|(i, &m)| format!("{i}:{:?}", cell_of(m))).collect();
    println!("  markers {}", markers.join(" "));
    let s = lvl.sunlight;
    println!("  sunlight start {} range {} inclination {}", s.shade_start, s.shade_range, s.inclination);
    let no_access = lvl.no_access.iter().filter(|&&c| c != 0).count();
    println!("  no-access cells {no_access}, access info set {}", lvl.access.iter().filter(|a| a.kind != 0).count());
    for t in lvl.things.iter().take(if all { usize::MAX } else { 10 }) {
        let angle = t.angle().map(|a| format!(" angle {a}")).unwrap_or_default();
        let data = match t.data() {
            ThingData::None => String::new(),
            d => format!(" {d:?}"),
        };
        println!("  #{:4} model {:3} kind {} owner {:3} at ({:5},{:5}){angle}{data}", t.slot + 1, t.model, t.kind, t.owner, t.x, t.z);
    }
}
