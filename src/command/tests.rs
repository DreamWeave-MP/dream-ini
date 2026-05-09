use super::*;
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::cli::CliCommand;
use crate::desktop_entry::APP_ID;
use clap_complete::Shell;

#[test]
fn run_with_generation_options_do_not_require_ini() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run_with_writers(
        Cli {
            verbose: false,
            help: None,
            version: None,
            ini: None,
            cfg: None,
            output: None,
            data_dir: None,
            data_local: None,
            resources: None,
            user_data: None,
            in_place: false,
            generate_completion: Some(Shell::Bash),
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            command: None,
        },
        &mut stdout,
        &mut stderr,
    )
    .unwrap();
    assert!(String::from_utf8(stdout).unwrap().contains("dream-ini"));
    assert!(stderr.is_empty());

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run_with_writers(
        Cli {
            verbose: false,
            help: None,
            version: None,
            ini: None,
            cfg: None,
            output: None,
            data_dir: None,
            data_local: None,
            resources: None,
            user_data: None,
            in_place: false,
            generate_completion: None,
            generate_manpage: true,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            command: None,
        },
        &mut stdout,
        &mut stderr,
    )
    .unwrap();
    let manpage = String::from_utf8(stdout).unwrap();
    assert!(manpage.contains("dream-ini"));
    assert!(manpage.contains("Import Morrowind.ini settings"));
    assert!(stderr.is_empty());
}

#[cfg(any(target_os = "linux", windows))]
#[test]
fn run_with_install_launcher_command_writes_launcher_and_icon() {
    let dir = unique_test_dir("install-launcher-run");
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    run_with_writers(
        Cli {
            verbose: false,
            help: None,
            version: None,
            ini: None,
            cfg: None,
            output: None,
            data_dir: None,
            data_local: None,
            resources: None,
            user_data: None,
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            command: Some(CliCommand::InstallLauncher {
                data_home: Some(dir.clone()),
            }),
        },
        &mut stdout,
        &mut stderr,
    )
    .unwrap();

    #[cfg(target_os = "linux")]
    let (launcher, icon) = (
        dir.join("applications").join(format!("{APP_ID}.desktop")),
        dir.join("icons/hicolor/512x512/apps")
            .join(format!("{APP_ID}.png")),
    );
    #[cfg(windows)]
    let (launcher, icon) = (
        dir.join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Dream INI.lnk"),
        dir.join("Dream INI").join("logo.ico"),
    );
    #[cfg(not(any(target_os = "linux", windows)))]
    let (launcher, icon) = (dir.join("unsupported"), dir.join("unsupported"));

    let stdout = String::from_utf8(stdout).unwrap();
    assert!(stdout.contains(&launcher.display().to_string()));
    assert!(stdout.contains(&icon.display().to_string()));
    #[cfg(target_os = "linux")]
    assert!(
        fs::read_to_string(launcher)
            .unwrap()
            .contains(&format!("Icon={APP_ID}"))
    );
    assert!(!fs::read(icon).unwrap().is_empty());
    assert!(stderr.is_empty());

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_writes_output_from_flag_paths() {
    let dir = unique_test_dir("flag-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    let output = dir.join("out.cfg");
    create_archive(&dir, "Morrowind.bsa");
    fs::write(
        &ini,
        "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n",
    )
    .unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();

    run_with(Cli {
        verbose: false,
        help: None,
        version: None,
        ini: Some(ini),
        cfg: Some(cfg),
        output: Some(output.clone()),
        data_dir: None,
        data_local: None,
        resources: None,
        user_data: None,
        in_place: false,
        generate_completion: None,
        generate_manpage: false,
        game_files: false,
        fonts: false,
        no_archives: false,
        encoding: None,
        command: None,
    })
    .unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains("encoding=win1252"));
    assert!(written.contains("no-sound=1"));
    assert!(written.contains("fallback=Movies_New_Game,intro.bik"));
    assert!(written.contains("fallback-archive=Morrowind.bsa"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_missing_cfg_does_not_create_input_cfg() {
    let dir = unique_test_dir("missing-cfg-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("missing.cfg");
    let output = dir.join("out.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

    let mut cli = import_cli(ini);
    cli.cfg = Some(cfg.clone());
    cli.output = Some(output.clone());
    cli.no_archives = true;
    run_with(cli).unwrap();

    assert!(!cfg.exists());
    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains("no-sound=1"));
    assert!(!written.contains("fallback-archive="));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_output_without_cfg_starts_empty() {
    let dir = unique_test_dir("output-no-cfg-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output = dir.join("out.cfg");
    fs::write(
        &ini,
        "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n",
    )
    .unwrap();

    let mut cli = import_cli(ini);
    cli.output = Some(output.clone());
    cli.no_archives = true;
    run_with(cli).unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains("encoding=win1252"));
    assert!(written.contains("no-sound=1"));
    assert!(written.contains("fallback=Movies_New_Game,intro.bik"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_without_output_writes_config_to_stdout_and_diagnostics_to_stderr() {
    let dir = unique_test_dir("default-stdout-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    create_archive(&dir, "Morrowind.bsa");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let mut cli = import_cli(ini);
    cli.no_archives = true;
    run_with_writers(cli, &mut stdout, &mut stderr).unwrap();

    let stdout = String::from_utf8(stdout).unwrap();
    let stderr = String::from_utf8(stderr).unwrap();
    assert!(stdout.contains("encoding=win1252"));
    assert!(stdout.contains("no-sound=1"));
    assert!(!stdout.contains("load ini file:"));
    assert!(stderr.contains("load ini file:"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_cfg_without_in_place_previews_to_stdout() {
    let dir = unique_test_dir("cfg-preview-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let mut cli = import_cli(ini);
    cli.cfg = Some(cfg.clone());
    cli.no_archives = true;
    run_with_writers(cli, &mut stdout, &mut stderr).unwrap();

    assert_eq!(fs::read_to_string(cfg).unwrap(), "encoding=win1252\n");
    assert!(String::from_utf8(stdout).unwrap().contains("no-sound=1"));
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("load cfg file:")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_rejects_multiple_output_modes() {
    let error = run_with(Cli {
        verbose: false,
        help: None,
        version: None,
        ini: Some(PathBuf::from("Morrowind.ini")),
        cfg: Some(PathBuf::from("openmw.cfg")),
        output: Some(PathBuf::from("openmw.cfg")),
        data_dir: None,
        data_local: None,
        resources: None,
        user_data: None,
        in_place: true,
        generate_completion: None,
        generate_manpage: false,
        game_files: false,
        fonts: false,
        no_archives: true,
        encoding: None,
        command: None,
    })
    .unwrap_err();

    match error {
        CliError::InvalidUsage(error) => assert!(error.contains("mutually exclusive")),
        CliError::MissingIni | CliError::Other(_) => panic!("expected invalid usage error"),
    }
}

#[test]
fn run_with_in_place_writes_cfg() {
    let dir = unique_test_dir("in-place-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(&cfg, "encoding=win1252\n").unwrap();

    let mut cli = import_cli(ini);
    cli.cfg = Some(cfg.clone());
    cli.in_place = true;
    cli.no_archives = true;
    run_with(cli).unwrap();

    assert!(fs::read_to_string(cfg).unwrap().contains("no-sound=1"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_singleton_path_options_clobbers_existing_values() {
    let dir = unique_test_dir("singleton-paths-run");
    fs::create_dir_all(&dir).unwrap();
    fs::create_dir_all(dir.join("resources")).unwrap();
    fs::write(dir.join("resources").join("version"), "installed").unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    let output = dir.join("out.cfg");
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

    let mut cli = import_cli(ini);
    cli.cfg = Some(cfg);
    cli.output = Some(output.clone());
    cli.data_local = Some(PathBuf::from("new-local"));
    cli.resources = Some(PathBuf::from("resources"));
    cli.user_data = Some(PathBuf::from("new-user-data"));
    cli.no_archives = true;
    run_with(cli).unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert_eq!(written.matches("data-local=").count(), 1);
    assert_eq!(written.matches("resources=").count(), 1);
    assert_eq!(written.matches("user-data=").count(), 1);
    assert!(written.contains("data-local=new-local\n"));
    assert!(written.contains("resources=resources\n"));
    assert!(written.contains("user-data=new-user-data\n"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_without_ini_returns_usage_error() {
    let mut cli = import_cli(PathBuf::from("unused.ini"));
    cli.ini = None;
    cli.output = Some(PathBuf::from("out.cfg"));
    let error = run_with(cli).unwrap_err();

    match error {
        CliError::InvalidUsage(error) => assert!(error.contains("--ini <FILE>")),
        CliError::MissingIni | CliError::Other(_) => panic!("expected invalid usage error"),
    }
}

#[test]
fn run_without_cfg_or_output_is_allowed() {
    let dir = unique_test_dir("no-cfg-no-output-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    create_archive(&dir, "Morrowind.bsa");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    run_with_writers(import_cli(ini), &mut stdout, &mut stderr).unwrap();

    assert!(String::from_utf8(stdout).unwrap().contains("no-sound=1"));
    assert!(
        String::from_utf8(stderr)
            .unwrap()
            .contains("load ini file:")
    );

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_missing_ini_returns_parity_error() {
    let dir = unique_test_dir("missing-ini-run");
    fs::create_dir_all(&dir).unwrap();
    let error = run_with(Cli {
        verbose: false,
        help: None,
        version: None,
        ini: Some(dir.join("missing.ini")),
        cfg: Some(dir.join("openmw.cfg")),
        output: None,
        data_dir: None,
        data_local: None,
        resources: None,
        user_data: None,
        in_place: true,
        generate_completion: None,
        generate_manpage: false,
        game_files: false,
        fonts: false,
        no_archives: false,
        encoding: None,
        command: None,
    })
    .unwrap_err();

    assert!(matches!(error, CliError::MissingIni));
    assert_eq!(MISSING_INI_EXIT_CODE, 253);

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_unsupported_encoding_returns_error() {
    let dir = unique_test_dir("bad-encoding-run");
    fs::create_dir_all(&dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
    fs::write(&cfg, "").unwrap();

    let mut cli = import_cli(ini);
    cli.cfg = Some(cfg);
    cli.in_place = true;
    cli.encoding = Some("bogus".to_owned());
    let error = run_with(cli).unwrap_err();

    match error {
        CliError::Other(error) => assert_eq!(error.to_string(), "unsupported encoding: bogus"),
        CliError::MissingIni | CliError::InvalidUsage(_) => {
            panic!("expected unsupported encoding error")
        }
    }

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_verbose_game_files_writes_content() {
    let dir = unique_test_dir("verbose-game-files-run");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let cfg = dir.join("openmw.cfg");
    let output = dir.join("out.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(
        &cfg,
        format!("data={}\nencoding=win1252\n", data_dir.display()),
    )
    .unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cli = import_cli(ini);
    cli.verbose = true;
    cli.cfg = Some(cfg);
    cli.output = Some(output.clone());
    cli.game_files = true;
    cli.no_archives = true;
    run_with(cli).unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains("content=Base.esm"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_output_only_game_files_searches_default_data_path() {
    let dir = unique_test_dir("output-only-game-files-run");
    let data_dir = dir.join("Data Files");
    fs::create_dir_all(&data_dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output = dir.join("out.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cli = import_cli(ini);
    cli.output = Some(output.clone());
    cli.game_files = true;
    cli.no_archives = true;
    run_with(cli).unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains(&format!("data={}\n", data_dir.display())));
    assert!(written.contains("content=Base.esm"));

    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn run_with_explicit_data_dir_writes_data_path() {
    let dir = unique_test_dir("explicit-data-dir-run");
    let data_dir = dir.join("External Data");
    fs::create_dir_all(&data_dir).unwrap();
    let ini = dir.join("Morrowind.ini");
    let output = dir.join("out.cfg");
    fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
    fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

    let mut cli = import_cli(ini);
    cli.output = Some(output.clone());
    cli.data_dir = Some(data_dir.clone());
    cli.game_files = true;
    cli.no_archives = true;
    run_with(cli).unwrap();

    let written = fs::read_to_string(output).unwrap();
    assert!(written.contains(&format!("data={}\n", data_dir.display())));
    assert!(written.contains("content=Base.esm"));

    fs::remove_dir_all(dir).unwrap();
}

fn import_cli(ini: PathBuf) -> Cli {
    Cli {
        verbose: false,
        help: None,
        version: None,
        ini: Some(ini),
        cfg: None,
        output: None,
        data_dir: None,
        data_local: None,
        resources: None,
        user_data: None,
        in_place: false,
        generate_completion: None,
        generate_manpage: false,
        game_files: false,
        fonts: false,
        no_archives: false,
        encoding: None,
        command: None,
    }
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
        "dream-ini-cli-{name}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
