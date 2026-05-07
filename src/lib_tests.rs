use super::*;
use crate::importer::import_archives;
use crate::test_support::{tes3_bytes, unique_test_dir, values};
use std::fs;
use std::path::Path;

mod parser;
mod plugin;

#[test]
fn imports_merge_fallback_and_archives() {
    let importer = IniImporter::new(ImportOptions::default());
    let mut cfg = parse_cfg_str("no-sound=0\nfallback=old\n");
    let ini = parse_ini_str(
        "[General]\nDisable Audio=1\nDisable Audio=0\n[Fonts]\nFont 0=magic\n[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n[Movies]\nNew Game=intro.bik\n",
    );

    let result = importer
        .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "no-sound"), &["0".to_owned()]);
    assert_eq!(
        values(&cfg, "fallback-archive"),
        &[
            "Morrowind.bsa".to_owned(),
            "Tribunal.bsa".to_owned(),
            "Bloodmoon.bsa".to_owned()
        ]
    );
    assert_eq!(
        values(&cfg, "fallback"),
        &["Movies_New_Game,intro.bik".to_owned()]
    );
    assert!(result.warnings.is_empty());
    assert!(result.events.is_empty());
}

#[test]
fn font_import_is_option_gated() {
    let ini = parse_ini_str("[Fonts]\nFont 0=magic\n[Movies]\nNew Game=intro.bik\n");
    let mut cfg = MultiMap::new();
    let importer = IniImporter::new(ImportOptions::default());
    let result = importer
        .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
        .unwrap();
    assert_eq!(
        values(&cfg, "fallback"),
        &["Movies_New_Game,intro.bik".to_owned()]
    );
    assert!(result.events.is_empty());

    let mut cfg = MultiMap::new();
    let importer = IniImporter::new(ImportOptions {
        import_fonts: true,
        ..ImportOptions::default()
    });
    let result = importer
        .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
        .unwrap();
    assert_eq!(
        values(&cfg, "fallback"),
        &[
            "Fonts_Font_0,magic".to_owned(),
            "Movies_New_Game,intro.bik".to_owned()
        ]
    );
    assert!(result.events.is_empty());
}

#[test]
fn archive_import_stops_at_first_missing_index() {
    let ini = parse_ini_str("[Archives]\nArchive 0=First.bsa\nArchive 2=Skipped.bsa\n");
    let mut cfg = MultiMap::new();
    import_archives(&mut cfg, &ini);
    assert_eq!(
        values(&cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned(), "First.bsa".to_owned()]
    );
}

#[test]
fn imports_game_files_using_tes3_dependencies() {
    let dir = unique_test_dir("game-files");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    fs::write(data_dir.join("Patch.esp"), tes3_bytes(&["Base.esm"])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Patch.esp\nGameFile1=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let result = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(
        values(&cfg, "content"),
        &["Base.esm".to_owned(), "Patch.esp".to_owned()]
    );
    assert!(result.events.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn verbose_game_file_import_reports_content_file_events() {
    let dir = unique_test_dir("game-files-verbose");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        verbose: true,
        ..ImportOptions::default()
    });

    let result = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(result.events.len(), 1);
    let ImportEvent::ContentFileResolved { path, .. } = &result.events[0] else {
        panic!("expected content file event");
    };
    assert_eq!(
        path.file_name().and_then(|name| name.to_str()),
        Some("Base.esm")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imports_game_files_from_default_data_files_path_and_writes_data() {
    let dir = unique_test_dir("game-files-default-data");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let result = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "data"),
        &[data_dir.to_string_lossy().into_owned()]
    );
    assert_eq!(result.events.len(), 1);
    assert_eq!(
        result.events[0],
        ImportEvent::DataDirAddedForContent {
            path: data_dir.clone()
        }
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_data_dir_is_written_when_it_resolves_content() {
    let dir = unique_test_dir("game-files-explicit-data");
    let data_dir = dir.join("External Data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![data_dir.clone()],
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "data"),
        &[data_dir.to_string_lossy().into_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn repeated_explicit_data_dirs_use_search_order() {
    let dir = unique_test_dir("game-files-explicit-data-order");
    let first_data_dir = dir.join("First Data");
    let second_data_dir = dir.join("Second Data");
    fs::create_dir_all(&first_data_dir).unwrap();
    fs::create_dir_all(&second_data_dir).unwrap();
    fs::write(second_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![first_data_dir, second_data_dir.clone()],
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "data"),
        &[second_data_dir.to_string_lossy().into_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn default_data_files_path_is_not_duplicated() {
    let dir = unique_test_dir("game-files-default-data-duplicate");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "data"),
        &[data_dir.to_string_lossy().into_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn default_data_files_path_is_not_added_when_config_data_resolves_content() {
    let dir = unique_test_dir("game-files-config-data-wins");
    let default_data_dir = dir.join("Data Files");
    let configured_data_dir = dir.join("Configured Data");
    fs::create_dir_all(&default_data_dir).unwrap();
    fs::create_dir_all(&configured_data_dir).unwrap();
    fs::write(default_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    fs::write(configured_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", configured_data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "data"),
        &[configured_data_dir.to_string_lossy().into_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_paths_are_relative_to_cfg_parent() {
    let dir = unique_test_dir("game-files-cfg-relative-data");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    let cfg = cfg_dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, "data=Data Files\n").unwrap();
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(values(&result.cfg, "data"), &["Data Files".to_owned()]);
    assert!(result.events.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_maps_uses_explicit_cfg_dir_for_relative_data_paths() {
    let dir = unique_test_dir("game-files-import-maps-cfg-dir");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str("data=Data Files\n");
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        cfg_dir: Some(cfg_dir),
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(values(&cfg, "data"), &["Data Files".to_owned()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_local_takes_precedence_over_cfg_data() {
    let dir = unique_test_dir("game-files-data-local-precedence");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    let local_dir = cfg_dir.join("Local Data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(data_dir.join("Patch.esp"), b"TES4").unwrap();
    fs::write(local_dir.join("Patch.esp"), tes3_bytes(&[])).unwrap();
    let cfg = cfg_dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, "data=Data Files\ndata-local=Local Data\n").unwrap();
    fs::write(&ini, "[Game Files]\nGameFile0=Patch.esp\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "content"), &["Patch.esp".to_owned()]);
    assert_eq!(values(&result.cfg, "data"), &["Data Files".to_owned()]);
    assert_eq!(
        values(&result.cfg, "data-local"),
        &["Local Data".to_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_local_takes_precedence_over_explicit_data() {
    let dir = unique_test_dir("game-files-data-local-over-explicit");
    let cfg_dir = dir.join("config");
    let explicit_data_dir = dir.join("Explicit Data");
    let local_dir = cfg_dir.join("Local Data");
    fs::create_dir_all(&explicit_data_dir).unwrap();
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(explicit_data_dir.join("Patch.esp"), b"TES4").unwrap();
    fs::write(local_dir.join("Patch.esp"), tes3_bytes(&[])).unwrap();
    let cfg = cfg_dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, "data-local=Local Data\n").unwrap();
    fs::write(&ini, "[Game Files]\nGameFile0=Patch.esp\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![explicit_data_dir],
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "content"), &["Patch.esp".to_owned()]);
    assert_eq!(values(&result.cfg, "data"), &[] as &[String]);
    assert_eq!(
        values(&result.cfg, "data-local"),
        &["Local Data".to_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_default_data_files_path_fails_import() {
    let dir = unique_test_dir("game-files-default-data-missing");
    fs::create_dir_all(&dir).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Missing.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err()
        .to_string();

    assert_eq!(values(&cfg, "content"), &[] as &[String]);
    assert_eq!(values(&cfg, "data"), &[] as &[String]);
    assert!(error.contains("content files not found: Missing.esm"));
    assert!(error.contains("pass --data or add data=..."));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_game_files_fail_import() {
    let dir = unique_test_dir("game-files-missing");
    fs::create_dir_all(&dir).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Missing.esp\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err()
        .to_string();

    assert_eq!(values(&cfg, "content"), &[] as &[String]);
    assert!(error.contains("content files not found: Missing.esp"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn partially_resolved_game_files_fail_without_writing_partial_content() {
    let dir = unique_test_dir("game-files-partial");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\nGameFile1=Missing.esp\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err()
        .to_string();

    assert_eq!(values(&cfg, "content"), &[] as &[String]);
    assert_eq!(values(&cfg, "data"), &[] as &[String]);
    assert!(error.contains("content files not found: Missing.esp"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn invalid_plugin_header_leaves_cfg_unchanged() {
    let dir = unique_test_dir("game-files-invalid-header-atomic");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Bad.esp"), b"TES4").unwrap();

    let mut cfg = parse_cfg_str("fallback=Old_Setting,old\nno-sound=0\n");
    let original_cfg = cfg.clone();
    let ini = parse_ini_str("[General]\nDisable Audio=1\n[Game Files]\nGameFile0=Bad.esp\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    assert!(matches!(error, ImportError::InvalidPluginHeader { .. }));
    assert_eq!(cfg, original_cfg);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_data_dir_is_not_written_when_data_local_already_covers_it() {
    let dir = unique_test_dir("game-files-explicit-covered-by-data-local");
    let data_dir = dir.join("External Data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data-local=\"{}\"\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![data_dir.clone()],
        ..ImportOptions::default()
    });

    let result = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(values(&cfg, "data"), &[] as &[String]);
    assert_eq!(
        values(&cfg, "data-local"),
        &[format!("\"{}\"", data_dir.display())]
    );
    assert!(result.events.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn duplicate_content_file_uses_first_search_path() {
    let dir = unique_test_dir("game-files-duplicate-search-precedence");
    let explicit_data_dir = dir.join("Explicit Data");
    let configured_data_dir = dir.join("Configured Data");
    fs::create_dir_all(&explicit_data_dir).unwrap();
    fs::create_dir_all(&configured_data_dir).unwrap();
    fs::write(explicit_data_dir.join("Patch.esp"), b"TES4").unwrap();
    fs::write(configured_data_dir.join("Patch.esp"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", configured_data_dir.display()));
    let original_cfg = cfg.clone();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Patch.esp\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![explicit_data_dir],
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    assert!(matches!(error, ImportError::InvalidPluginHeader { .. }));
    assert_eq!(cfg, original_cfg);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn sparse_game_file_indices_are_imported() {
    let dir = unique_test_dir("game-files-sparse");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    fs::write(data_dir.join("Patch.esp"), tes3_bytes(&["Base.esm"])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\nGameFile2=Patch.esp\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(
        values(&cfg, "content"),
        &["Base.esm".to_owned(), "Patch.esp".to_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn game_file_values_are_trimmed_before_resolution() {
    let dir = unique_test_dir("game-files-trimmed");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm \n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn content_file_paths_are_rejected() {
    let dir = unique_test_dir("game-files-path-entry");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=../Data Files/Base.esm \n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err()
        .to_string();

    assert!(error.contains("invalid content file name: ../Data Files/Base.esm"));
    assert_eq!(values(&cfg, "content"), &[] as &[String]);
    assert_eq!(values(&cfg, "data"), &[] as &[String]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn failed_game_file_import_leaves_cfg_unchanged() {
    let dir = unique_test_dir("game-files-error-atomic");
    fs::create_dir_all(&dir).unwrap();

    let mut cfg = parse_cfg_str("fallback=Old_Setting,old\nno-sound=0\n");
    let original_cfg = cfg.clone();
    let ini = parse_ini_str(
        "[General]\nDisable Audio=1\n[Weather]\nSunrise Time=6\n[Game Files]\nGameFile0=Missing.esm\n",
    );
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    assert_eq!(cfg, original_cfg);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_paths_preserves_existing_cfg_and_writes_imports() {
    let dir = unique_test_dir("path-import");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    let output = dir.join("imported.cfg");
    fs::write(
        &cfg,
        "no-sound=0\nfallback=Old_Setting,old\nencoding=win1252\n",
    )
    .unwrap();
    fs::write(
        &ini,
        "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n[Archives]\nArchive 0=Tribunal.bsa\n",
    )
    .unwrap();

    let importer = IniImporter::new(ImportOptions::default());
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
    assert_eq!(values(&result.cfg, "encoding"), &["win1252".to_owned()]);
    assert_eq!(
        values(&result.cfg, "fallback"),
        &["Movies_New_Game,intro.bik".to_owned()]
    );
    assert_eq!(
        values(&result.cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned(), "Tribunal.bsa".to_owned()]
    );

    fs::write(&output, serialize_cfg(&result.cfg)).unwrap();
    let written = fs::read_to_string(&output).unwrap();
    assert!(written.contains("no-sound=1"));
    assert!(written.contains("fallback=Movies_New_Game,intro.bik"));
    assert!(written.contains("fallback-archive=Morrowind.bsa"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_paths_writes_exact_golden_output() {
    let dir = unique_test_dir("golden-output");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(
        &cfg,
        "resources=resources\nno-sound=0\nfallback=Old_Setting,old\n",
    )
    .unwrap();
    fs::write(
        &ini,
        "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n[Archives]\nArchive 0=Tribunal.bsa\n",
    )
    .unwrap();

    let importer = IniImporter::new(ImportOptions::default());
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(
        serialize_cfg(&result.cfg),
        concat!(
            "encoding=win1252\n",
            "fallback=Movies_New_Game,intro.bik\n",
            "fallback-archive=Morrowind.bsa\n",
            "fallback-archive=Tribunal.bsa\n",
            "no-sound=1\n",
            "resources=resources\n",
        )
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_options_set_singleton_paths() {
    let dir = unique_test_dir("singleton-path-options");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(
        &cfg,
        concat!(
            "data-local=old-local\n",
            "data-local=other-local\n",
            "resources=old-resources\n",
            "userdata=old-userdata\n",
        ),
    )
    .unwrap();
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        data_local: Some(PathBuf::from("new-local")),
        resources: Some(PathBuf::from("new-resources")),
        userdata: Some(PathBuf::from("new-userdata")),
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "data-local"), &["new-local".to_owned()]);
    assert_eq!(
        values(&result.cfg, "resources"),
        &["new-resources".to_owned()]
    );
    assert_eq!(
        values(&result.cfg, "userdata"),
        &["new-userdata".to_owned()]
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_paths_does_not_include_composed_synthetic_entries() {
    let dir = unique_test_dir("user-output-only");
    let resources = dir.join("resources");
    fs::create_dir_all(resources.join("vfs")).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, format!("resources={}\n", resources.display())).unwrap();
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let importer = IniImporter::new(ImportOptions::default());
    let result = importer.import_paths(&ini, &cfg).unwrap();
    let output = serialize_cfg(&result.cfg);

    assert!(output.contains("resources="));
    assert!(output.contains("no-sound=1"));
    assert!(!output.contains("data="));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn imports_fallback_values_with_legacy_shapes() {
    let dir = unique_test_dir("fallback-shapes");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    let output = dir.join("imported.cfg");
    fs::write(&cfg, "encoding=win1252\n").unwrap();
    fs::write(
        &ini,
        concat!(
            "[Movies]\n",
            "New Game=movie,with,commas.bik\n",
            "[Weather]\n",
            "Sunrise Time=6\n",
            "Sun Glare Fader Max=0.75\n",
            "[Weather Clear]\n",
            "Sky Day Color=10,20,30\n",
        ),
    )
    .unwrap();

    let importer = IniImporter::new(ImportOptions::default());
    let result = importer.import_paths(&ini, &cfg).unwrap();
    fs::write(&output, serialize_cfg(&result.cfg)).unwrap();
    let written = fs::read_to_string(&output).unwrap();

    assert!(written.contains("fallback=Movies_New_Game,movie,with,commas.bik"));
    assert!(written.contains("fallback=Weather_Sunrise_Time,6"));
    assert!(written.contains("fallback=Weather_Sun_Glare_Fader_Max,0.75"));
    assert!(written.contains("fallback=Weather_Clear_Sky_Day_Color,10,20,30"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_paths_missing_cfg_starts_empty() {
    let dir = unique_test_dir("import-paths-missing-cfg");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("missing.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let importer = IniImporter::new(ImportOptions::default());
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert!(!cfg.exists());
    assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
    assert_eq!(values(&result.cfg, "encoding"), &["win1252".to_owned()]);

    fs::remove_dir_all(dir).unwrap();
}
