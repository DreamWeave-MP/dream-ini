use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::MultiMap;

pub(crate) fn values<'a>(map: &'a MultiMap, key: &str) -> &'a [String] {
    map.get(key).map_or(&[], Vec::as_slice)
}

pub(crate) fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
    let masters: Vec<_> = masters.iter().map(|master| master.as_bytes()).collect();
    tes3_bytes_from_master_bytes(&masters)
}

pub(crate) fn tes3_bytes_from_master_bytes(masters: &[&[u8]]) -> Vec<u8> {
    let mut record = Vec::new();
    subrecord(&mut record, *b"HEDR", &[0; 300]);
    for master in masters {
        let mut name = (*master).to_vec();
        name.push(0);
        subrecord(&mut record, *b"MAST", &name);
        subrecord(&mut record, *b"DATA", &0u64.to_le_bytes());
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TES3");
    bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record);
    bytes
}

pub(crate) fn subrecord(output: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
    output.extend_from_slice(&name);
    output.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    output.extend_from_slice(data);
}

pub(crate) fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "dream-ini-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
