use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use rome_ini::{ImportOptions, IniImporter, TextEncoding};

const MISSING_INI_EXIT_CODE: u8 = 253;

#[derive(Debug, Parser)]
#[allow(clippy::struct_excessive_bools)]
#[command(
    name = "rome-ini",
    about = "Import Morrowind.ini settings into openmw.cfg",
    override_usage = "rome-ini <options> inifile [configfile]"
)]
struct Cli {
    /// Verbose output
    #[arg(short, long)]
    verbose: bool,

    /// Morrowind.ini file
    #[arg(short, long, value_name = "FILE")]
    ini: Option<PathBuf>,

    /// openmw.cfg file
    #[arg(short, long, value_name = "FILE")]
    cfg: Option<PathBuf>,

    /// Output openmw.cfg file
    #[arg(short, long, value_name = "FILE")]
    output: Option<PathBuf>,

    /// Import esm and esp files
    #[arg(short = 'g', long = "game-files")]
    game_files: bool,

    /// Import bitmap fonts
    #[arg(short, long)]
    fonts: bool,

    /// Disable bsa archives import
    #[arg(short = 'A', long = "no-archives")]
    no_archives: bool,

    /// Character encoding used in `OpenMW` game messages: win1250, win1251, or win1252
    #[arg(short, long, value_name = "ENCODING")]
    encoding: Option<String>,

    /// Positional Morrowind.ini file
    #[arg(value_name = "inifile")]
    positional_ini: Option<PathBuf>,

    /// Positional openmw.cfg file
    #[arg(value_name = "configfile")]
    positional_cfg: Option<PathBuf>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::MissingIni) => {
            eprintln!("ini file does not exist");
            ExitCode::from(MISSING_INI_EXIT_CODE)
        }
        Err(CliError::Other(error)) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug)]
enum CliError {
    MissingIni,
    Other(Box<dyn std::error::Error>),
}

impl<E> From<E> for CliError
where
    E: std::error::Error + 'static,
{
    fn from(error: E) -> Self {
        Self::Other(Box::new(error))
    }
}

fn run() -> Result<(), CliError> {
    let cli = Cli::parse();
    run_with(cli)
}

fn run_with(cli: Cli) -> Result<(), CliError> {
    let Some(ini_path) = cli.ini.or(cli.positional_ini) else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };
    let cfg_path = cli.cfg.or(cli.positional_cfg);
    let output_path = if let Some(output) = cli.output {
        output
    } else if let Some(cfg_path) = &cfg_path {
        cfg_path.clone()
    } else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

    if !ini_path.exists() {
        return Err(CliError::MissingIni);
    }

    if let Some(cfg_path) = &cfg_path
        && !cfg_path.exists()
    {
        eprintln!("cfg file does not exist");
    }

    let encoding = cli
        .encoding
        .as_deref()
        .map(TextEncoding::parse)
        .transpose()?;
    let options = ImportOptions {
        import_game_files: cli.game_files,
        import_fonts: cli.fonts,
        import_archives: !cli.no_archives,
        encoding,
        verbose: cli.verbose,
        ..ImportOptions::default()
    };

    let importer = IniImporter::new(options);

    if let Some(cfg_path) = &cfg_path {
        println!("load cfg file: {}", cfg_path.display());
    }
    println!("load ini file: {}", ini_path.display());

    let result = importer.import_optional_cfg_path(&ini_path, cfg_path.as_deref())?;
    for message in &result.messages {
        println!("{message}");
    }
    for warning in &result.warnings {
        eprintln!("Warning: {warning}");
    }

    println!("write to: {}", output_path.display());
    importer.save_config_output(&output_path, &result.cfg)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_positional_ini_and_cfg() {
        let cli = Cli::parse_from(["rome-ini", "Morrowind.ini", "openmw.cfg"]);

        assert_eq!(cli.positional_ini, Some(PathBuf::from("Morrowind.ini")));
        assert_eq!(cli.positional_cfg, Some(PathBuf::from("openmw.cfg")));
        assert!(cli.ini.is_none());
        assert!(cli.cfg.is_none());
    }

    #[test]
    fn accepts_flag_ini_and_cfg() {
        let cli = Cli::parse_from(["rome-ini", "--ini", "mw.ini", "--cfg", "openmw.cfg"]);

        assert_eq!(cli.ini, Some(PathBuf::from("mw.ini")));
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
        assert!(cli.positional_ini.is_none());
        assert!(cli.positional_cfg.is_none());
    }

    #[test]
    fn flag_paths_take_precedence_over_positionals() {
        let cli = Cli::parse_from([
            "rome-ini",
            "positional.ini",
            "positional.cfg",
            "--ini",
            "flag.ini",
            "--cfg",
            "flag.cfg",
        ]);

        let ini_path = cli.ini.or(cli.positional_ini);
        let cfg_path = cli.cfg.or(cli.positional_cfg);

        assert_eq!(ini_path, Some(PathBuf::from("flag.ini")));
        assert_eq!(cfg_path, Some(PathBuf::from("flag.cfg")));
    }

    #[test]
    fn parses_import_options() {
        let cli = Cli::parse_from([
            "rome-ini",
            "--game-files",
            "--fonts",
            "--no-archives",
            "--encoding",
            "win1251",
            "--output",
            "out.cfg",
            "mw.ini",
            "openmw.cfg",
        ]);

        assert!(cli.game_files);
        assert!(cli.fonts);
        assert!(cli.no_archives);
        assert_eq!(cli.encoding.as_deref(), Some("win1251"));
        assert_eq!(cli.output, Some(PathBuf::from("out.cfg")));
    }

    #[test]
    fn help_mentions_core_options() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("--game-files"));
        assert!(help.contains("--fonts"));
        assert!(help.contains("--no-archives"));
        assert!(help.contains("--encoding"));
        assert!(help.contains("--output"));
    }

    #[test]
    fn run_with_writes_output_from_flag_paths() {
        let dir = unique_test_dir("flag-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        let cfg = dir.join("openmw.cfg");
        let output = dir.join("out.cfg");
        fs::write(
            &ini,
            "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n",
        )
        .unwrap();
        fs::write(&cfg, "encoding=win1252\n").unwrap();

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: Some(cfg),
            output: Some(output.clone()),
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
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

        run_with(Cli {
            verbose: false,
            ini: None,
            cfg: None,
            output: Some(output.clone()),
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: Some(ini),
            positional_cfg: Some(cfg.clone()),
        })
        .unwrap();

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

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        let written = fs::read_to_string(output).unwrap();
        assert!(written.contains("encoding=win1252"));
        assert!(written.contains("no-sound=1"));
        assert!(written.contains("fallback=Movies_New_Game,intro.bik"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_without_cfg_or_output_prints_help() {
        let dir = unique_test_dir("no-cfg-no-output-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: None,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_with_missing_ini_returns_parity_error() {
        let dir = unique_test_dir("missing-ini-run");
        fs::create_dir_all(&dir).unwrap();
        let error = run_with(Cli {
            verbose: false,
            ini: Some(dir.join("missing.ini")),
            cfg: Some(dir.join("openmw.cfg")),
            output: None,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
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

        let error = run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: Some(cfg),
            output: None,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: Some("bogus".to_owned()),
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap_err();

        match error {
            CliError::Other(error) => assert_eq!(error.to_string(), "unsupported encoding: bogus"),
            CliError::MissingIni => panic!("expected unsupported encoding error"),
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

        run_with(Cli {
            verbose: true,
            ini: Some(ini),
            cfg: Some(cfg),
            output: Some(output.clone()),
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        let written = fs::read_to_string(output).unwrap();
        assert!(written.contains("content=Base.esm"));

        fs::remove_dir_all(dir).unwrap();
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
            "rome-ini-cli-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
