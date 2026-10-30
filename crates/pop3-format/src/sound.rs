//! Original sound banks ("SDT"): see docs/specs/sound.md. Parsing only - no playback, no MPEG decoding,
//! no CC0 replacement sounds (a separate, much larger item: `docs/specs/sound.md`'s own TODO neighbour).

use crate::level::LevelError;
use std::ops::Range;
use std::path::Path;

const HEADER_SIZE: usize = 40;

fn u32_at(data: &[u8], at: usize) -> Result<u32, LevelError> {
    data.get(at..at + 4).map(|b| u32::from_le_bytes(b.try_into().expect("4 bytes"))).ok_or(LevelError::BadSize { expected: at + 4, got: data.len() })
}

fn u16_at(data: &[u8], at: usize) -> Result<u16, LevelError> {
    data.get(at..at + 2).map(|b| u16::from_le_bytes(b.try_into().expect("2 bytes"))).ok_or(LevelError::BadSize { expected: at + 2, got: data.len() })
}

fn u8_at(data: &[u8], at: usize) -> Result<u8, LevelError> {
    data.get(at).copied().ok_or(LevelError::BadSize { expected: at + 1, got: data.len() })
}

/// NUL-padded ASCII name, trimmed (sound.md "Entry header": 16 bytes for SFX, 8 for music).
fn name_at(data: &[u8], at: usize, len: usize) -> Result<String, LevelError> {
    let bytes = data.get(at..at + len).ok_or(LevelError::BadSize { expected: at + len, got: data.len() })?;
    Ok(String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string())
}

/// One SFX/voice/drum entry: a 40-byte header (sound.md "Entry header") followed by `data_size` bytes of
/// sample data. `data` is a byte range into the bank's own buffer (not copied: banks are large).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub sample_rate: u16,
    pub bits: u8,
    pub flags: u8,
    pub data: Range<usize>,
    /// Loop start/end, **in bytes** of decoded PCM from the start of the data (sound.md "Loops": not
    /// samples - the loop end in samples would run past the end of the data). `(0, 0)` means no loop.
    pub loop_range: Option<(u32, u32)>,
}

impl Entry {
    /// Flag `0x01` (sound.md "Flags"): interleaved L/R when set, one channel when clear.
    pub fn is_stereo(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Flag `0x10`: an unanalysed codec, only seen in `soundd2low.SDT`. The spec says to use `Soundd2`
    /// and ignore the low bank rather than decode this.
    pub fn is_compressed(&self) -> bool {
        self.flags & 0x10 != 0
    }

    /// Flag `0x20`: an MPEG (music) entry - never set in a `SoundBank` parsed from an SFX file, present
    /// here only so a caller checking raw flags on an arbitrary entry sees the same bit `MusicEntry` does.
    pub fn is_music(&self) -> bool {
        self.flags & 0x20 != 0
    }

    /// Flag `0x80`: do not play (sound.md "Flags") - zero-sized in `soundd2low`, or (in a drum bank)
    /// entry 0, whose declared data range overlaps entry 1's header and data.
    pub fn is_placeholder(&self) -> bool {
        self.flags & 0x80 != 0
    }

    /// Raw signed 16-bit little-endian PCM samples (sound.md "Sample data"), interleaved L/R if
    /// `is_stereo()`. Returned interleaved rather than split per channel: a caller already has
    /// `is_stereo()` to de-interleave with, and most consumers (a WAV header, a Bevy `AudioSource`) want
    /// the interleaved form back anyway.
    pub fn pcm(&self, bank_data: &[u8]) -> Vec<i16> {
        bank_data[self.data.clone()].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect()
    }
}

fn parse_entry(data: &[u8], offset: usize) -> Result<Entry, LevelError> {
    let header_size = u32_at(data, offset)? as usize;
    if header_size != HEADER_SIZE {
        return Err(LevelError::BadFormat(format!("sound entry at {offset}: header size {header_size}, expected {HEADER_SIZE}")));
    }
    let data_size = u32_at(data, offset + 4)? as usize;
    let name = name_at(data, offset + 8, 16)?;
    let sample_rate = u16_at(data, offset + 24)?;
    let bits = u8_at(data, offset + 26)?;
    let flags = u8_at(data, offset + 27)?;
    let loop_start = u32_at(data, offset + 32)?;
    let loop_end = u32_at(data, offset + 36)?;
    let start = offset + HEADER_SIZE;
    let end = start.checked_add(data_size).ok_or(LevelError::BadSize { expected: data_size, got: data.len() })?;
    if end > data.len() {
        return Err(LevelError::BadSize { expected: end, got: data.len() });
    }
    let loop_range = if loop_start == 0 && loop_end == 0 { None } else { Some((loop_start, loop_end)) };
    Ok(Entry { name, sample_rate, bits, flags, data: start..end, loop_range })
}

/// `n x u32` offset table, then each entry at its own table-given offset (sound.md "Container") - not
/// assumed back-to-back, so a drum bank's overlapping placeholder entry 0 (sound.md "Flags") parses the
/// same way as any other bank.
#[derive(Clone, Debug)]
pub struct SoundBank {
    pub entries: Vec<Entry>,
}

impl SoundBank {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let n = u32_at(data, 0)? as usize;
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let offset = u32_at(data, 4 + i * 4)? as usize;
            entries.push(parse_entry(data, offset)?);
        }
        Ok(SoundBank { entries })
    }

    /// Opens `name` (e.g. `"Soundd2.sdt"`) case-insensitively under `data_dir` (sound.md: "the file-name
    /// case varies").
    pub fn load(data_dir: &Path, name: &str) -> Result<Self, LevelError> {
        let path = crate::find_file(data_dir, name).unwrap_or_else(|| data_dir.join(name));
        Self::parse(&std::fs::read(path)?)
    }
}

/// One `PopDrones22.SDT` track: same container, a shorter 8-byte name, and its MPEG Layer II payload left
/// unparsed (sound.md "Music" - decoding Layer II is a codec, not a format-parsing concern).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusicEntry {
    pub name: String,
    pub sample_rate: u16,
    pub flags: u8,
    /// Decoded 16-bit stereo PCM size in bytes once a Layer II decoder is wired up - the stored payload
    /// (`mpeg_data`) is still-encoded and a different (smaller) size.
    pub decoded_size: u32,
    pub data: Range<usize>,
}

impl MusicEntry {
    /// The raw MPEG-2 (LSF) Layer II byte stream (sound.md "Music"): undecoded.
    pub fn mpeg_data<'a>(&self, bank_data: &'a [u8]) -> &'a [u8] {
        &bank_data[self.data.clone()]
    }
}

fn parse_music_entry(data: &[u8], offset: usize) -> Result<MusicEntry, LevelError> {
    let header_size = u32_at(data, offset)? as usize;
    if header_size != HEADER_SIZE {
        return Err(LevelError::BadFormat(format!("music entry at {offset}: header size {header_size}, expected {HEADER_SIZE}")));
    }
    let data_size = u32_at(data, offset + 4)? as usize;
    let name = name_at(data, offset + 8, 8)?;
    let sample_rate = u16_at(data, offset + 24)?;
    let flags = u8_at(data, offset + 27)?;
    let decoded_size = u32_at(data, offset + 32)?;
    let start = offset + HEADER_SIZE;
    let end = start.checked_add(data_size).ok_or(LevelError::BadSize { expected: data_size, got: data.len() })?;
    if end > data.len() {
        return Err(LevelError::BadSize { expected: end, got: data.len() });
    }
    Ok(MusicEntry { name, sample_rate, flags, decoded_size, data: start..end })
}

/// `PopDrones22.SDT`'s own container: same offset table as `SoundBank`, `MusicEntry` headers.
#[derive(Clone, Debug)]
pub struct MusicBank {
    pub entries: Vec<MusicEntry>,
}

impl MusicBank {
    pub fn parse(data: &[u8]) -> Result<Self, LevelError> {
        let n = u32_at(data, 0)? as usize;
        let mut entries = Vec::with_capacity(n);
        for i in 0..n {
            let offset = u32_at(data, 4 + i * 4)? as usize;
            entries.push(parse_music_entry(data, offset)?);
        }
        Ok(MusicBank { entries })
    }

    pub fn load(data_dir: &Path, name: &str) -> Result<Self, LevelError> {
        let path = crate::find_file(data_dir, name).unwrap_or_else(|| data_dir.join(name));
        Self::parse(&std::fs::read(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(name: &[u8], name_len: usize, data_size: u32, sample_rate: u16, bits: u8, flags: u8, field32: u32, field36: u32) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_SIZE];
        h[0..4].copy_from_slice(&(HEADER_SIZE as u32).to_le_bytes());
        h[4..8].copy_from_slice(&data_size.to_le_bytes());
        h[8..8 + name.len().min(name_len)].copy_from_slice(&name[..name.len().min(name_len)]);
        h[24..26].copy_from_slice(&sample_rate.to_le_bytes());
        h[26] = bits;
        h[27] = flags;
        h[32..36].copy_from_slice(&field32.to_le_bytes());
        h[36..40].copy_from_slice(&field36.to_le_bytes());
        h
    }

    /// Builds a container (offset table + each entry's header+data back to back) from `(header, data)` pairs.
    fn container(entries: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
        let table_end = 4 + 4 * entries.len();
        let mut offsets = Vec::new();
        let mut body = Vec::new();
        let mut at = table_end;
        for (h, d) in entries {
            offsets.push(at as u32);
            body.extend_from_slice(h);
            body.extend_from_slice(d);
            at += h.len() + d.len();
        }
        let mut out = (entries.len() as u32).to_le_bytes().to_vec();
        for o in offsets {
            out.extend_from_slice(&o.to_le_bytes());
        }
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn parses_mono_and_stereo_entries_with_loops_and_placeholders() {
        let fire_pcm: Vec<u8> = (0..8i16).flat_map(i16::to_le_bytes).collect();
        let ambience_pcm: Vec<u8> = vec![0u8; 16]; // 4 stereo frames
        let entries = vec![
            (header(b"fire", 16, fire_pcm.len() as u32, 22050, 16, 0x02, 24058, 70386), fire_pcm.clone()),
            (header(b"amb_sea1", 16, ambience_pcm.len() as u32, 22050, 16, 0x03, 0, 0), ambience_pcm.clone()),
            (header(b"placeholder", 16, 0, 11025, 16, 0x92, 0, 0), Vec::new()),
        ];
        let bank_data = container(&entries);
        let bank = SoundBank::parse(&bank_data).unwrap();
        assert_eq!(bank.entries.len(), 3);

        let fire = &bank.entries[0];
        assert_eq!(fire.name, "fire");
        assert!(!fire.is_stereo() && !fire.is_placeholder() && !fire.is_compressed());
        assert_eq!(fire.loop_range, Some((24058, 70386)), "sound.md's own fire loop points, in bytes");
        assert_eq!(fire.pcm(&bank_data), (0..8i16).collect::<Vec<_>>());

        let amb = &bank.entries[1];
        assert!(amb.is_stereo());
        assert_eq!(amb.loop_range, None, "(0, 0) means no loop");

        let placeholder = &bank.entries[2];
        assert!(placeholder.is_placeholder());
    }

    #[test]
    fn drum_bank_placeholder_entry_overlapping_the_next_one_parses_independently() {
        // sound.md "Drum banks": entry 0 is `0x83`, its data range is entry 1's own header + data shifted
        // by 40 bytes (overlapping it) - the parser must not assume entries are back to back when reading
        // data ranges, only use each entry's own table offset and header-given size.
        let real_data = vec![7u8; 64];
        let real_header = header(b"0a", 16, real_data.len() as u32, 22050, 16, 0x03, 0, 0);
        let placeholder_data_size = (HEADER_SIZE + real_data.len()) as u32;
        let placeholder_header = header(b"0a", 16, placeholder_data_size, 22050, 16, 0x83, 0, 0);

        let table_end = (4 + 4 * 2) as u32;
        let offset0 = table_end; // entry 0's own 40-byte header sits right after the offset table
        let offset1 = offset0 + HEADER_SIZE as u32; // entry 1's header follows it directly

        let mut bank_data = 2u32.to_le_bytes().to_vec();
        bank_data.extend_from_slice(&offset0.to_le_bytes());
        bank_data.extend_from_slice(&offset1.to_le_bytes());
        bank_data.extend_from_slice(&placeholder_header);
        bank_data.extend_from_slice(&real_header);
        bank_data.extend_from_slice(&real_data);

        let bank = SoundBank::parse(&bank_data).unwrap();
        assert_eq!(bank.entries.len(), 2);
        assert!(bank.entries[0].is_placeholder());
        assert!(!bank.entries[1].is_placeholder());
        assert_eq!(bank.entries[0].data.len(), HEADER_SIZE + real_data.len(), "entry 0's declared data spans entry 1's own header+data");
        assert_eq!(bank.entries[1].pcm(&bank_data).len(), real_data.len() / 2, "entry 1 still parses its own real data correctly");
    }

    #[test]
    fn rejects_a_truncated_bank() {
        assert!(SoundBank::parse(&[1, 0, 0, 0]).is_err(), "claims one entry but the offset table is missing");
        assert!(SoundBank::parse(&[]).is_err());
    }

    #[test]
    fn parses_a_music_entry_with_its_shorter_header() {
        let mpeg: Vec<u8> = vec![0xff, 0xf5, 0xb0, 0x54, 0, 0, 0, 0];
        let h = header(b"idea11", 8, mpeg.len() as u32, 22050, 16, 0x25, 20321278, 0);
        let bank_data = container(&[(h, mpeg.clone())]);
        let bank = MusicBank::parse(&bank_data).unwrap();
        assert_eq!(bank.entries.len(), 1);
        let idea11 = &bank.entries[0];
        assert_eq!(idea11.name, "idea11");
        assert_eq!(idea11.sample_rate, 22050);
        assert_eq!(idea11.decoded_size, 20321278);
        assert_eq!(idea11.mpeg_data(&bank_data), mpeg.as_slice());
    }
}
