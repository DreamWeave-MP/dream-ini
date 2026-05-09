use std::fs;

use crate::test_support::{tes3_bytes, unique_test_dir, values};
use crate::{
    ImportError, ImportEvent, ImportOptions, IniImporter, MultiMap, parse_cfg_str, parse_ini_str,
};

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
fn shared_content_and_archive_data_dir_is_written_once() {
    let dir = unique_test_dir("game-files-archives-shared-data");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    fs::write(data_dir.join("Morrowind.bsa"), []).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
    assert_eq!(
        values(&cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned()]
    );
    assert_eq!(
        values(&cfg, "data"),
        &[data_dir.to_string_lossy().into_owned()]
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
    assert_eq!(
        values(&result.cfg, "data"),
        &[data_dir.display().to_string()]
    );
    assert!(result.events.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_maps_uses_explicit_cfg_dir_for_relative_data_paths() {
    let dir = unique_test_dir("game-files-import-maps-cfg-dir");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    let local_dir = cfg_dir.join("Local Data");
    let resources_dir = cfg_dir.join("resources");
    let user_data_dir = cfg_dir.join("user-data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&local_dir).unwrap();
    fs::create_dir_all(&resources_dir).unwrap();
    fs::create_dir_all(&user_data_dir).unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str(
        "data=Data Files\ndata-local=Local Data\nresources=resources\nuser-data=user-data\n",
    );
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
    assert_eq!(values(&cfg, "data-local"), &["Local Data".to_owned()]);
    assert_eq!(values(&cfg, "resources"), &["resources".to_owned()]);
    assert_eq!(values(&cfg, "user-data"), &["user-data".to_owned()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_resources_vfs_is_not_used_for_morrowind_content_import() {
    let dir = unique_test_dir("game-files-resources-vfs");
    let cfg_dir = dir.join("config");
    let resources = cfg_dir.join("resources");
    let vfs = resources.join("vfs");
    fs::create_dir_all(&vfs).unwrap();
    fs::write(resources.join("version"), "installed").unwrap();
    fs::write(vfs.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cfg = parse_cfg_str("resources=resources\n");
    let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        cfg_dir: Some(cfg_dir),
        ..ImportOptions::default()
    });

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    assert_eq!(values(&cfg, "resources"), &["resources".to_owned()]);
    assert_eq!(values(&cfg, "content"), &[] as &[String]);
    assert_eq!(values(&cfg, "data"), &[] as &[String]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_local_does_not_take_precedence_over_cfg_data() {
    let dir = unique_test_dir("game-files-data-local-ignored-over-data");
    let cfg_dir = dir.join("config");
    let data_dir = cfg_dir.join("Data Files");
    let local_dir = cfg_dir.join("Local Data");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(data_dir.join("Patch.esp"), tes3_bytes(&[])).unwrap();
    fs::write(local_dir.join("Patch.esp"), b"TES4").unwrap();
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
    assert_eq!(
        values(&result.cfg, "data"),
        &[data_dir.display().to_string()]
    );
    assert_eq!(
        values(&result.cfg, "data-local"),
        &[local_dir.display().to_string()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_local_does_not_take_precedence_over_explicit_data() {
    let dir = unique_test_dir("game-files-data-local-over-explicit");
    let cfg_dir = dir.join("config");
    let explicit_data_dir = dir.join("Explicit Data");
    let local_dir = cfg_dir.join("Local Data");
    fs::create_dir_all(&explicit_data_dir).unwrap();
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(explicit_data_dir.join("Patch.esp"), tes3_bytes(&[])).unwrap();
    fs::write(local_dir.join("Patch.esp"), b"TES4").unwrap();
    let cfg = cfg_dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, "data-local=Local Data\n").unwrap();
    fs::write(&ini, "[Game Files]\nGameFile0=Patch.esp\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        data_dirs: vec![explicit_data_dir.clone()],
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert_eq!(values(&result.cfg, "content"), &["Patch.esp".to_owned()]);
    assert_eq!(
        values(&result.cfg, "data"),
        &[explicit_data_dir.display().to_string()]
    );
    assert_eq!(
        values(&result.cfg, "data-local"),
        &[local_dir.display().to_string()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_data_local_is_not_used_as_only_content_source() {
    let dir = unique_test_dir("game-files-data-local-only-ignored");
    let cfg_dir = dir.join("config");
    let local_dir = cfg_dir.join("Local Data");
    fs::create_dir_all(&local_dir).unwrap();
    fs::write(local_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
    let cfg = cfg_dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    fs::write(&cfg, "data-local=Local Data\n").unwrap();
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();

    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });
    let error = importer.import_paths(&ini, &cfg).unwrap_err().to_string();

    assert!(error.contains("content files not found: Base.esm"));
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
fn explicit_data_dir_is_written_when_data_local_already_covers_it() {
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
    assert_eq!(
        values(&cfg, "data"),
        &[data_dir.to_string_lossy().into_owned()]
    );
    assert_eq!(
        values(&cfg, "data-local"),
        &[format!("\"{}\"", data_dir.display())]
    );
    assert_eq!(result.events.len(), 1);
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
fn game_file_indices_sort_numerically_and_preserve_duplicate_order() {
    let dir = unique_test_dir("game-files-numeric-order");
    fs::create_dir_all(&dir).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str(concat!(
        "[Game Files]\n",
        "GameFile10=Ten.esp\n",
        "GameFile2=Two.esp\n",
        "GameFile0=Zero.esm\n",
        "GameFileFoo=Ignored.esp\n",
        "GameFile-1=IgnoredToo.esp\n",
        "GameFile 1=Spaced.esp\n",
        "GameFile0=ZeroPatch.esp\n",
    ));
    let importer = IniImporter::new(ImportOptions {
        import_game_files: true,
        import_archives: false,
        ..ImportOptions::default()
    });

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    match error {
        ImportError::MissingContentFiles { files, .. } => {
            assert_eq!(
                files,
                vec![
                    "Zero.esm".to_owned(),
                    "ZeroPatch.esp".to_owned(),
                    "Two.esp".to_owned(),
                    "Ten.esp".to_owned(),
                ]
            );
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(values(&cfg, "content"), &[] as &[String]);
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
fn archive_paths_are_rejected_without_mutating_cfg() {
    let dir = unique_test_dir("archives-path-entry");
    fs::create_dir_all(&dir).unwrap();

    let mut cfg = parse_cfg_str("fallback-archive=old.bsa\n");
    let ini = parse_ini_str("[Archives]\nArchive 0=../Data Files/Tribunal.bsa\n");
    let importer = IniImporter::new(ImportOptions::default());

    let error = importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap_err();

    assert!(matches!(
        error,
        ImportError::InvalidArchiveName(file) if file == "../Data Files/Tribunal.bsa"
    ));
    assert_eq!(values(&cfg, "fallback-archive"), &["old.bsa".to_owned()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn archive_values_are_trimmed_and_match_suffix_case_insensitively() {
    let dir = unique_test_dir("archives-trimmed-case");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join("Morrowind.bsa"), []).unwrap();
    fs::write(data_dir.join("Tribunal.BSA"), []).unwrap();

    let mut cfg = MultiMap::new();
    let ini = parse_ini_str("[Archives]\nArchive 0=Tribunal.BSA \n");
    let importer = IniImporter::new(ImportOptions::default());

    importer
        .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
        .unwrap();

    assert_eq!(
        values(&cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned(), "Tribunal.BSA".to_owned()]
    );
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
