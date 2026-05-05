use std::fs;
use std::path::PathBuf;
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
        .args(["--game-files", "--no-archives", "--output"])
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
    assert!(stderr.contains("pass --data-dir or add data=..."));

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
        .args(["--game-files", "--no-archives", "--data-dir"])
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
fn stdout_mode_keeps_config_output_clean() {
    let dir = unique_test_dir("stdout-mode");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--stdout", "--no-archives", "--ini"])
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
fn dry_run_reports_without_writing_output() {
    let dir = unique_test_dir("dry-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output_cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--dry-run", "--no-archives", "--output"])
        .arg(&output_cfg)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    assert!(!output_cfg.exists());
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .contains("dry run: not writing output")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_mode_outputs_structured_result_to_stdout() {
    let dir = unique_test_dir("json-mode");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--json", "--no-archives", "--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["cfg"]["encoding"][0], "win1252");
    assert_eq!(json["cfg"]["no-sound"][0], "1");
    assert!(json["text"].as_str().unwrap().contains("no-sound=1\n"));
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("load ini file:")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn json_conflicts_with_stdout() {
    let output = Command::new(BIN)
        .args(["--json", "--stdout"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("cannot be used")
    );
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
fn missing_output_mode_fails_with_usage_error() {
    let dir = unique_test_dir("missing-output-mode");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let output = Command::new(BIN)
        .args(["--ini"])
        .arg(&ini)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("--cfg <FILE>"));
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn generates_bash_completion_to_stdout() {
    let output = Command::new(BIN)
        .args(["--generate-completion", "bash"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("dream-ini"));
    assert!(stdout.contains("--game-files"));
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
}

#[test]
fn generates_manpage_to_stdout() {
    let output = Command::new(BIN)
        .arg("--generate-manpage")
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("dream-ini"));
    assert!(stdout.contains("Import Morrowind.ini settings"));
    assert!(stdout.contains("Import mode requires"));
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

fn unique_test_dir(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "dream-ini-integration-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
