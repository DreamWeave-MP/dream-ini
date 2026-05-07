use std::fs;
use std::path::{Path, PathBuf};

use crate::test_support::{unique_test_dir, values};
use crate::{
    ImportError, ImportOptions, IniImporter, MultiMap, parse_cfg_str, parse_ini_str, serialize_cfg,
};

#[test]
fn imports_merge_fallback_and_archives() {
    let dir = unique_test_dir("merge-fallback-archives");
    create_archives(&dir, &["Morrowind.bsa", "Tribunal.bsa", "Bloodmoon.bsa"]);
    let ini_path = dir.join("Morrowind.ini");
    let importer = IniImporter::new(ImportOptions::default());
    let mut cfg = parse_cfg_str("no-sound=0\nfallback=old\n");
    let ini = parse_ini_str(
        "[General]\nDisable Audio=1\nDisable Audio=0\n[Fonts]\nFont 0=magic\n[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n[Movies]\nNew Game=intro.bik\n",
    );

    let result = importer.import_maps(&mut cfg, &ini, &ini_path).unwrap();

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
    assert_eq!(result.events.len(), 1);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn font_import_is_option_gated() {
    let ini = parse_ini_str("[Fonts]\nFont 0=magic\n[Movies]\nNew Game=intro.bik\n");
    let mut cfg = MultiMap::new();
    let importer = IniImporter::new(ImportOptions {
        import_archives: false,
        ..ImportOptions::default()
    });
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
        import_archives: false,
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
    let dir = unique_test_dir("archive-gap");
    create_archives(&dir, &["Morrowind.bsa", "First.bsa"]);
    let ini_path = dir.join("Morrowind.ini");
    let ini = parse_ini_str("[Archives]\nArchive 0=First.bsa\nArchive 2=Skipped.bsa\n");
    let mut cfg = MultiMap::new();
    let importer = IniImporter::new(ImportOptions::default());
    importer.import_maps(&mut cfg, &ini, &ini_path).unwrap();
    assert_eq!(
        values(&cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned(), "First.bsa".to_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn archive_import_writes_data_dir_used_to_resolve_archives() {
    let dir = unique_test_dir("archive-data-dir");
    create_archives(&dir, &["Morrowind.bsa", "Tribunal.bsa"]);
    let ini_path = dir.join("Morrowind.ini");
    let ini = parse_ini_str("[Archives]\nArchive 0=Tribunal.bsa\n");
    let mut cfg = MultiMap::new();
    let importer = IniImporter::new(ImportOptions::default());

    importer.import_maps(&mut cfg, &ini, &ini_path).unwrap();

    assert_eq!(
        values(&cfg, "data"),
        &[dir.join("Data Files").display().to_string()]
    );
    assert_eq!(
        values(&cfg, "fallback-archive"),
        &["Morrowind.bsa".to_owned(), "Tribunal.bsa".to_owned()]
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_archive_import_leaves_cfg_unchanged() {
    let dir = unique_test_dir("missing-archive");
    create_archives(&dir, &["Morrowind.bsa"]);
    let ini_path = dir.join("Morrowind.ini");
    let ini = parse_ini_str("[Archives]\nArchive 0=Tribunal.bsa\n");
    let mut cfg = parse_cfg_str("fallback-archive=old.bsa\n");
    let importer = IniImporter::new(ImportOptions::default());

    let error = importer.import_maps(&mut cfg, &ini, &ini_path).unwrap_err();

    match error {
        ImportError::MissingArchives { files, .. } => {
            assert_eq!(files, vec!["Tribunal.bsa".to_owned()]);
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(values(&cfg, "fallback-archive"), &["old.bsa".to_owned()]);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn import_paths_preserves_existing_cfg_and_writes_imports() {
    let dir = unique_test_dir("path-import");
    fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("openmw.cfg");
    let ini = dir.join("Morrowind.ini");
    create_archives(&dir, &["Morrowind.bsa", "Tribunal.bsa"]);
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
    create_archives(&dir, &["Morrowind.bsa", "Tribunal.bsa"]);
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
        format!(
            concat!(
                "data={}\n",
                "encoding=win1252\n",
                "fallback=Movies_New_Game,intro.bik\n",
                "fallback-archive=Morrowind.bsa\n",
                "fallback-archive=Tribunal.bsa\n",
                "no-sound=1\n",
                "resources=resources\n",
            ),
            dir.join("Data Files").display()
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
        import_archives: false,
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

    let importer = IniImporter::new(ImportOptions {
        import_archives: false,
        ..ImportOptions::default()
    });
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

    let importer = IniImporter::new(ImportOptions {
        import_archives: false,
        ..ImportOptions::default()
    });
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

    let importer = IniImporter::new(ImportOptions {
        import_archives: false,
        ..ImportOptions::default()
    });
    let result = importer.import_paths(&ini, &cfg).unwrap();

    assert!(!cfg.exists());
    assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
    assert_eq!(values(&result.cfg, "encoding"), &["win1252".to_owned()]);

    fs::remove_dir_all(dir).unwrap();
}

fn create_archives(dir: &Path, archives: &[&str]) {
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    for archive in archives {
        fs::write(data_dir.join(archive), []).unwrap();
    }
}
