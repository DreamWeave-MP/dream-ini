use std::fs;

use crate::plugin::{apply_morrowind_expansion_order, dependency_sort};
use crate::test_support::{tes3_bytes, tes3_bytes_from_master_bytes, unique_test_dir};
use crate::{ImportError, PluginFormat, TextEncoding, read_plugin_header};

#[test]
fn dependency_sort_places_masters_before_dependents() {
    let sorted = dependency_sort(vec![
        ("Patch.esp".to_owned(), vec!["Base.esm".to_owned()]),
        ("Base.esm".to_owned(), vec![]),
    ]);
    assert_eq!(sorted, vec!["Base.esm".to_owned(), "Patch.esp".to_owned()]);
}

#[test]
fn applies_morrowind_expansion_order() {
    let mut files = vec![
        "Morrowind.esm".to_owned(),
        "Bloodmoon.esm".to_owned(),
        "Tribunal.esm".to_owned(),
    ];
    apply_morrowind_expansion_order(&mut files);
    assert_eq!(
        files,
        vec!["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]
    );
}

#[test]
fn reads_tes3_header_masters() {
    let dir = unique_test_dir("tes3-header");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Patch.esp");
    fs::write(&plugin, tes3_bytes(&["Morrowind.esm", "Tribunal.esm"])).unwrap();

    let header = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

    assert_eq!(header.name, "Patch.esp");
    assert_eq!(header.masters, vec!["Morrowind.esm", "Tribunal.esm"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn reads_tes3_header_masters_with_selected_encoding() {
    let dir = unique_test_dir("tes3-header-encoding");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Patch.esp");
    fs::write(&plugin, tes3_bytes_from_master_bytes(&[b"caf\xe9.esm"])).unwrap();

    let header = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

    assert_eq!(header.masters, vec!["caf\u{e9}.esm"]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_invalid_tes3_header() {
    let dir = unique_test_dir("tes3-invalid");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Bad.esp");
    fs::write(&plugin, b"TES4").unwrap();

    let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap_err();
    assert!(matches!(error, ImportError::InvalidPluginHeader { .. }));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_truncated_tes3_record() {
    let dir = unique_test_dir("tes3-truncated-record");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Bad.esp");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TES3");
    bytes.extend_from_slice(&8u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    fs::write(&plugin, bytes).unwrap();

    let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
        .unwrap_err()
        .to_string();
    assert!(error.contains("TES3 record extends past end of file"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_truncated_tes3_subrecord() {
    let dir = unique_test_dir("tes3-truncated-subrecord");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Bad.esp");
    let mut record = Vec::new();
    record.extend_from_slice(b"MAST");
    record.extend_from_slice(&8u32.to_le_bytes());
    record.extend_from_slice(b"short");
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TES3");
    bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record);
    fs::write(&plugin, bytes).unwrap();

    let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
        .unwrap_err()
        .to_string();
    assert!(error.contains("subrecord extends past TES3 record"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rejects_truncated_non_master_tes3_subrecord_data() {
    let dir = unique_test_dir("tes3-truncated-non-master-data");
    fs::create_dir_all(&dir).unwrap();
    let plugin = dir.join("Bad.esp");
    let mut record = Vec::new();
    record.extend_from_slice(b"HEDR");
    record.extend_from_slice(&300u32.to_le_bytes());
    record.extend_from_slice(&[0; 8]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"TES3");
    bytes.extend_from_slice(&308u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record);
    fs::write(&plugin, bytes).unwrap();

    let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
        .unwrap_err()
        .to_string();
    assert!(error.contains("TES3 record extends past end of file"));
    fs::remove_dir_all(dir).unwrap();
}
