use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const BIN: &str = env!("CARGO_BIN_EXE_dream-ini");

#[test]
fn version_prints_package_version() {
    let output = Command::new(BIN).arg("--version").output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("dream-ini {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn default_data_files_search_imports_content_and_writes_data() {
    let dir = unique_test_dir("default-data-files");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output_cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let output = Command::new(BIN)
        .args(["--game-files", "--no-archives", "-o"])
        .arg(&output_cfg)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    let written = fs::read_to_string(&output_cfg).unwrap();
    assert!(written.contains("content=Base.esm\n"));
    assert!(written.contains(&format!("data={}\n", data_dir.display())));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn missing_game_file_fails_without_writing_output() {
    let dir = unique_test_dir("missing-game-file");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output_cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Missing.esp\n").unwrap();

    let output = Command::new(BIN)
        .args(["--game-files", "--no-archives", "--output"])
        .arg(&output_cfg)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!output_cfg.exists());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("content files not found: Missing.esp"));
    assert!(stderr.contains("pass --data or add data=..."));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn explicit_data_dir_imports_content_and_writes_data() {
    let dir = unique_test_dir("explicit-data-dir");
    let install_dir = dir.join("install");
    let data_dir = install_dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::create_dir_all(dir.join("ini-source")).unwrap();
    let ini = dir.join("ini-source").join("Morrowind.ini");
    let output_cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let output = Command::new(BIN)
        .args(["--game-files", "--no-archives", "--data"])
        .arg(&data_dir)
        .args(["--output"])
        .arg(&output_cfg)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    let written = fs::read_to_string(&output_cfg).unwrap();
    assert!(written.contains("content=Base.esm\n"));
    assert!(written.contains(&format!("data={}\n", data_dir.display())));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn singleton_path_options_replace_existing_values() {
    let dir = unique_test_dir("singleton-path-options");
    let resources = dir.join("resources");
    fs::create_dir_all(&resources).unwrap();
    fs::write(resources.join("version"), "installed").unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    let output_cfg = dir.join("imported.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(
        &cfg,
        concat!(
            "data-local=old-local\n",
            "data-local=other-local\n",
            "resources=old-resources\n",
            "user-data=old-user-data\n",
        ),
    )
    .unwrap();

    let output = Command::new(BIN)
        .args(["--no-archives", "--ini"])
        .arg(&ini)
        .args(["--cfg"])
        .arg(&cfg)
        .args(["--output"])
        .arg(&output_cfg)
        .args(["-l", "new-local", "-r", "resources", "-u", "new-user-data"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let written = fs::read_to_string(output_cfg).unwrap();
    assert_eq!(written.matches("data-local=").count(), 1);
    assert_eq!(written.matches("resources=").count(), 1);
    assert_eq!(written.matches("user-data=").count(), 1);
    assert!(written.contains("data-local=new-local\n"));
    assert!(written.contains("resources=resources\n"));
    assert!(written.contains("user-data=new-user-data\n"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn default_stdout_keeps_config_output_clean() {
    let dir = unique_test_dir("default-stdout");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--no-archives", "--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stdout.contains("encoding=win1252\n"));
    assert!(stdout.contains("no-sound=1\n"));
    assert!(!stdout.contains("load ini file:"));
    assert!(stderr.contains("load ini file:"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn cfg_without_in_place_prints_without_writing_cfg() {
    let dir = unique_test_dir("cfg-preview");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();

    let output = Command::new(BIN)
        .args(["--no-archives", "--ini"])
        .arg(&ini)
        .args(["--cfg"])
        .arg(&cfg)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(fs::read_to_string(&cfg).unwrap(), "encoding=win1252\n");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("encoding=win1252\n"));
    assert!(stdout.contains("no-sound=1\n"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn in_place_writes_back_to_cfg() {
    let dir = unique_test_dir("in-place");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();

    let output = Command::new(BIN)
        .args(["-w", "--no-archives", "--ini"])
        .arg(&ini)
        .args(["--cfg"])
        .arg(&cfg)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("write to:"));
    let written = fs::read_to_string(&cfg).unwrap();
    assert!(written.contains("encoding=win1252\n"));
    assert!(written.contains("no-sound=1\n"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn output_conflicts_with_in_place() {
    let output = Command::new(BIN)
        .args([
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "--output",
            "out.cfg",
            "--in-place",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot be used")
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

#[test]
fn in_place_requires_cfg() {
    let output = Command::new(BIN)
        .args(["--ini", "Morrowind.ini", "--in-place"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("--cfg"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

#[test]
fn missing_ini_fails_with_usage_error() {
    let output = Command::new(BIN)
        .args(["--output", "openmw.cfg"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr).unwrap().contains("--ini"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

#[test]
fn missing_output_mode_defaults_to_stdout() {
    let dir = unique_test_dir("missing-output-mode");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    create_archive(&dir, "Morrowind.bsa");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("no-sound=1\n")
    );
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("load ini file:")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generation_mode_rejects_import_arguments() {
    let output = Command::new(BIN)
        .args([
            "--generate-manpage",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot be used")
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
}

#[test]
fn generates_bash_completion_to_stdout() {
    let output = Command::new(BIN).args(["-C", "bash"]).output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("dream-ini"));
    // clap_complete emits a static option inventory for bash. Clap still rejects invalid mixed
    // generation/import modes at parse time; see generation_mode_rejects_import_arguments.
    assert!(stdout.contains("--game-files"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn generates_manpage_to_stdout() {
    let output = Command::new(BIN).arg("-M").output().unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("dream-ini"));
    assert!(stdout.contains("Import Morrowind.ini settings"));
    assert!(stdout.contains("--ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]"));
    assert!(stdout.contains("--generate-completion <SHELL>"));
    assert!(stdout.contains("--generate-manpage"));
    assert!(stdout.contains("Import mode requires"));
    assert!(stdout.contains("do not require"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
    let mut record = Vec::new();
    subrecord(&mut record, *b"HEDR", &[0; 300]);
    for master in masters {
        let mut name = master.as_bytes().to_vec();
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

fn subrecord(output: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
    output.extend_from_slice(&name);
    output.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
    output.extend_from_slice(data);
}

fn create_archive(dir: &Path, archive: &str) {
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    fs::write(data_dir.join(archive), []).unwrap();
}

fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "dream-ini-integration-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
