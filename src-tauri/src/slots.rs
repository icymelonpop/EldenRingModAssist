//! Read the PC BND4 USER_DATA010 character summary. The rest of the save stays unopened.
use md5::{Digest, Md5};
use serde::Serialize;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

#[derive(Serialize, Debug)]
pub struct Slot {
    slot: usize,
    name: String,
    level: u32,
    seconds: u32,
}

const SUMMARY_OFFSET: usize = 0x1954;
const RECORD_SIZE: usize = 0x24c;
const SLOT_COUNT: usize = 10;
const SECTION_SIZE: usize = 0x60000;
const UNSUPPORTED: &str = "지원하는 PC 세이브의 캐릭터 목록 형식을 확인할 수 없습니다. 파일 백업은 계속 사용할 수 있습니다.";

fn summary(data: &[u8]) -> Result<Vec<Slot>, String> {
    let slice = data
        .get(SUMMARY_OFFSET..SUMMARY_OFFSET + SLOT_COUNT * (1 + RECORD_SIZE))
        .ok_or(UNSUPPORTED)?;
    let (active, records) = slice.split_at(SLOT_COUNT);
    if active.iter().any(|v| *v > 1) {
        return Err(UNSUPPORTED.into());
    }
    let mut slots = Vec::new();
    for (i, record) in records.chunks_exact(RECORD_SIZE).enumerate() {
        if active[i] == 0 {
            continue;
        }
        let utf16: Vec<u16> = record[..32]
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|ch| *ch != 0)
            .collect();
        let name = String::from_utf16(&utf16).map_err(|_| UNSUPPORTED)?;
        let number = |off: usize| u32::from_le_bytes(record[off..off + 4].try_into().unwrap());
        slots.push(Slot {
            slot: i + 1,
            name,
            level: number(0x22),
            seconds: number(0x26),
        });
    }
    Ok(slots)
}

pub fn read(path: &Path) -> Result<Vec<Slot>, String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    if !ext.eq_ignore_ascii_case("sl2") && !ext.eq_ignore_ascii_case("co2") {
        return Err(UNSUPPORTED.into());
    }
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0; 64];
    file.read_exact(&mut header).map_err(|_| UNSUPPORTED)?;
    let integer = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap());
    if &header[..4] != b"BND4"
        || integer(&header[12..16]) != 12
        || u64::from_le_bytes(header[16..24].try_into().unwrap()) != 64
        || u64::from_le_bytes(header[32..40].try_into().unwrap()) != 32
    {
        return Err(UNSUPPORTED.into());
    }
    file.seek(SeekFrom::Start(64 + 10 * 32))
        .map_err(|e| e.to_string())?;
    let mut entry = [0; 32];
    file.read_exact(&mut entry).map_err(|_| UNSUPPORTED)?;
    let size = u64::from_le_bytes(entry[8..16].try_into().unwrap());
    let data_offset = integer(&entry[16..20]) as u64;
    let name_offset = integer(&entry[20..24]) as u64;
    if size != (SECTION_SIZE + 16) as u64
        || data_offset + size > file.metadata().map_err(|e| e.to_string())?.len()
    {
        return Err(UNSUPPORTED.into());
    }
    file.seek(SeekFrom::Start(name_offset))
        .map_err(|e| e.to_string())?;
    let mut identifier = [0; 26];
    file.read_exact(&mut identifier).map_err(|_| UNSUPPORTED)?;
    let expected: Vec<u8> = "USER_DATA010\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    if identifier.as_slice() != expected {
        return Err(UNSUPPORTED.into());
    }
    file.seek(SeekFrom::Start(data_offset))
        .map_err(|e| e.to_string())?;
    let mut checksum = [0; 16];
    let mut data = vec![0; SECTION_SIZE];
    file.read_exact(&mut checksum)
        .and_then(|_| file.read_exact(&mut data))
        .map_err(|_| UNSUPPORTED)?;
    if Md5::digest(&data).as_slice() != checksum {
        return Err(
            "캐릭터 요약 영역의 체크섬이 일치하지 않습니다. 저장이 끝난 후 다시 확인하세요.".into(),
        );
    }
    summary(&data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_slots_preserve_full_level_and_korean_name() {
        let mut data = vec![0; SECTION_SIZE];
        data[SUMMARY_OFFSET + 9] = 1;
        let start = SUMMARY_OFFSET + SLOT_COUNT + 9 * RECORD_SIZE;
        let name: Vec<_> = "빛바랜 자"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        data[start..start + name.len()].copy_from_slice(&name);
        data[start + 0x22..start + 0x26].copy_from_slice(&448u32.to_le_bytes());
        data[start + 0x26..start + 0x2a].copy_from_slice(&1231312u32.to_le_bytes());
        let slots = summary(&data).unwrap();
        assert_eq!(
            (slots.len(), slots[0].slot, slots[0].level, slots[0].seconds),
            (1, 10, 448, 1231312)
        );
        assert_eq!(slots[0].name, "빛바랜 자");
        assert!(summary(&data[..20]).is_err());
    }
}
