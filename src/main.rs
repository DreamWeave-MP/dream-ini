use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use clap_complete::Shell;
use dream_ini::{ImportOptions, ImportResult, IniImporter, TextEncoding, serialize_cfg};
use serde::Serialize;

const MISSING_INI_EXIT_CODE: u8 = 253;

#[derive(Debug, Parser)]
#[allow(clippy::struct_excessive_bools)]
#[command(
    name = "dream-ini",
    about = "Import Morrowind.ini settings into openmw.cfg",
    version,
    override_usage = "dream-ini <options> inifile [configfile]"
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

    /// Data Files directory to search before cfg/default data paths
    #[arg(long = "data-dir", visible_alias = "data", value_name = "DIR")]
    data_dirs: Vec<PathBuf>,

    /// Parse and report without writing an output file
    #[arg(long)]
    dry_run: bool,

    /// Write resulting cfg to stdout instead of a file
    #[arg(long = "stdout", visible_alias = "print", conflicts_with = "json")]
    stdout: bool,

    /// Write import result JSON to stdout instead of a file
    #[arg(long, conflicts_with = "stdout")]
    json: bool,

    /// Generate shell completion script to stdout
    #[arg(long, value_name = "SHELL", conflicts_with = "generate_manpage")]
    generate_completion: Option<Shell>,

    /// Generate roff manpage to stdout
    #[arg(long, conflicts_with = "generate_completion")]
    generate_manpage: bool,

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
    let mut stdout = io::stdout().lock();
    let mut stderr = io::stderr().lock();
    run_with_writers(cli, &mut stdout, &mut stderr)
}

#[cfg(test)]
fn run_with(cli: Cli) -> Result<(), CliError> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    run_with_writers(cli, &mut stdout, &mut stderr)
}

fn run_with_writers(
    cli: Cli,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if handle_generated_output(&cli, stdout)? {
        return Ok(());
    }
    let stdout_mode = cli.stdout || cli.json;
    let Some(ini_path) = cli.ini.or(cli.positional_ini) else {
        write!(stdout, "{}", Cli::command().render_help())?;
        return Ok(());
    };
    let cfg_path = cli.cfg.or(cli.positional_cfg);
    let output_path = cli.output.clone().or_else(|| {
        (!stdout_mode && !cli.dry_run && !cli.json)
            .then(|| cfg_path.clone())
            .flatten()
    });
    if output_path.is_none() && !stdout_mode && !cli.dry_run && !cli.json {
        write!(stdout, "{}", Cli::command().render_help())?;
        return Ok(());
    }

    if !ini_path.exists() {
        return Err(CliError::MissingIni);
    }

    if let Some(cfg_path) = &cfg_path
        && !cfg_path.exists()
    {
        writeln!(stderr, "cfg file does not exist")?;
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
        data_dirs: cli.data_dirs,
        encoding,
        verbose: cli.verbose,
        ..ImportOptions::default()
    };

    let importer = IniImporter::new(options);

    if let Some(cfg_path) = &cfg_path {
        diagnostic(
            stdout_mode,
            stdout,
            stderr,
            format_args!("load cfg file: {}", cfg_path.display()),
        )?;
    }
    diagnostic(
        stdout_mode,
        stdout,
        stderr,
        format_args!("load ini file: {}", ini_path.display()),
    )?;

    let result = importer.import_optional_cfg_path(&ini_path, cfg_path.as_deref())?;
    for message in &result.messages {
        diagnostic(stdout_mode, stdout, stderr, format_args!("{message}"))?;
    }
    for warning in &result.warnings {
        writeln!(stderr, "Warning: {warning}")?;
    }

    write_result_output(
        &importer,
        &result,
        OutputMode {
            json: cli.json,
            stdout: cli.stdout,
            dry_run: cli.dry_run,
            output_path,
        },
        stdout,
        stderr,
    )?;

    Ok(())
}

fn handle_generated_output(cli: &Cli, stdout: &mut dyn Write) -> Result<bool, CliError> {
    if let Some(shell) = cli.generate_completion {
        clap_complete::generate(shell, &mut Cli::command(), "dream-ini", stdout);
        return Ok(true);
    }

    if cli.generate_manpage {
        clap_mangen::Man::new(Cli::command()).render(stdout)?;
        return Ok(true);
    }

    Ok(false)
}

#[derive(Debug)]
struct OutputMode {
    json: bool,
    stdout: bool,
    dry_run: bool,
    output_path: Option<PathBuf>,
}

fn write_result_output(
    importer: &IniImporter,
    result: &ImportResult,
    mode: OutputMode,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if mode.json {
        let output = JsonOutput {
            cfg: &result.cfg,
            text: serialize_cfg(&result.cfg),
            warnings: &result.warnings,
            messages: &result.messages,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
    } else if mode.stdout {
        write!(stdout, "{}", serialize_cfg(&result.cfg))?;
    } else if let Some(output_path) = mode.output_path {
        diagnostic(
            false,
            stdout,
            stderr,
            format_args!("write to: {}", output_path.display()),
        )?;
        if mode.dry_run {
            diagnostic(
                false,
                stdout,
                stderr,
                format_args!("dry run: not writing output"),
            )?;
        } else {
            importer.save_config_output(&output_path, &result.cfg)?;
        }
    } else if mode.dry_run {
        diagnostic(
            false,
            stdout,
            stderr,
            format_args!("dry run: not writing output"),
        )?;
    }

    Ok(())
}

#[derive(Debug, Serialize)]
struct JsonOutput<'a> {
    cfg: &'a dream_ini::MultiMap,
    text: String,
    warnings: &'a [String],
    messages: &'a [String],
}

fn diagnostic(
    stdout_mode: bool,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
    args: std::fmt::Arguments<'_>,
) -> io::Result<()> {
    if stdout_mode {
        writeln!(stderr, "{args}")
    } else {
        writeln!(stdout, "{args}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn accepts_positional_ini_and_cfg() {
        let cli = Cli::parse_from(["dream-ini", "Morrowind.ini", "openmw.cfg"]);

        assert_eq!(cli.positional_ini, Some(PathBuf::from("Morrowind.ini")));
        assert_eq!(cli.positional_cfg, Some(PathBuf::from("openmw.cfg")));
        assert!(cli.ini.is_none());
        assert!(cli.cfg.is_none());
    }

    #[test]
    fn accepts_flag_ini_and_cfg() {
        let cli = Cli::parse_from(["dream-ini", "--ini", "mw.ini", "--cfg", "openmw.cfg"]);

        assert_eq!(cli.ini, Some(PathBuf::from("mw.ini")));
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
        assert!(cli.positional_ini.is_none());
        assert!(cli.positional_cfg.is_none());
    }

    #[test]
    fn flag_paths_take_precedence_over_positionals() {
        let cli = Cli::parse_from([
            "dream-ini",
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
            "dream-ini",
            "--game-files",
            "--fonts",
            "--no-archives",
            "--encoding",
            "win1251",
            "--data-dir",
            "Data Files",
            "--data",
            "Alt Data",
            "--dry-run",
            "--stdout",
            "--output",
            "out.cfg",
            "mw.ini",
            "openmw.cfg",
        ]);

        assert!(cli.game_files);
        assert!(cli.fonts);
        assert!(cli.no_archives);
        assert!(cli.dry_run);
        assert!(cli.stdout);
        assert_eq!(
            cli.data_dirs,
            vec![PathBuf::from("Data Files"), PathBuf::from("Alt Data")]
        );
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
        assert!(help.contains("--data-dir"));
        assert!(help.contains("--dry-run"));
        assert!(help.contains("--stdout"));
        assert!(help.contains("--json"));
        assert!(help.contains("--generate-completion"));
        assert!(help.contains("--generate-manpage"));
    }

    #[test]
    fn parses_generation_options() {
        let completion = Cli::parse_from(["dream-ini", "--generate-completion", "bash"]);
        assert_eq!(completion.generate_completion, Some(Shell::Bash));

        let manpage = Cli::parse_from(["dream-ini", "--generate-manpage"]);
        assert!(manpage.generate_manpage);
    }

    #[test]
    fn run_with_json_writes_result_to_stdout() {
        let dir = unique_test_dir("json-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_writers(
            Cli {
                verbose: false,
                ini: Some(ini),
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                dry_run: false,
                stdout: false,
                json: true,
                generate_completion: None,
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: true,
                encoding: None,
                positional_ini: None,
                positional_cfg: None,
            },
            &mut stdout,
            &mut stderr,
        )
        .unwrap();

        let json: serde_json::Value = serde_json::from_slice(&stdout).unwrap();
        assert_eq!(json["cfg"]["encoding"][0], "win1252");
        assert_eq!(json["cfg"]["no-sound"][0], "1");
        assert!(json["text"].as_str().unwrap().contains("no-sound=1\n"));
        assert!(
            String::from_utf8(stderr)
                .unwrap()
                .contains("load ini file:")
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_with_generation_options_do_not_require_ini() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        run_with_writers(
            Cli {
                verbose: false,
                ini: None,
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                dry_run: false,
                stdout: false,
                json: false,
                generate_completion: Some(Shell::Bash),
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: false,
                encoding: None,
                positional_ini: None,
                positional_cfg: None,
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
                ini: None,
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                dry_run: false,
                stdout: false,
                json: false,
                generate_completion: None,
                generate_manpage: true,
                game_files: false,
                fonts: false,
                no_archives: false,
                encoding: None,
                positional_ini: None,
                positional_cfg: None,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
    fn run_with_stdout_writes_config_to_stdout_and_diagnostics_to_stderr() {
        let dir = unique_test_dir("stdout-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_writers(
            Cli {
                verbose: false,
                ini: Some(ini),
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                dry_run: false,
                stdout: true,
                json: false,
                generate_completion: None,
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: true,
                encoding: None,
                positional_ini: None,
                positional_cfg: None,
            },
            &mut stdout,
            &mut stderr,
        )
        .unwrap();

        let stdout = String::from_utf8(stdout).unwrap();
        let stderr = String::from_utf8(stderr).unwrap();
        assert!(stdout.contains("encoding=win1252"));
        assert!(stdout.contains("no-sound=1"));
        assert!(!stdout.contains("load ini file:"));
        assert!(stderr.contains("load ini file:"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_with_dry_run_does_not_write_output() {
        let dir = unique_test_dir("dry-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        let output = dir.join("out.cfg");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            dry_run: true,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        assert!(!output.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_with_dry_run_without_output_is_allowed() {
        let dir = unique_test_dir("dry-run-no-output");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: None,
            data_dirs: Vec::new(),
            dry_run: true,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
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

    #[test]
    fn run_with_output_only_game_files_searches_default_data_path() {
        let dir = unique_test_dir("output-only-game-files-run");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        let output = dir.join("out.cfg");
        fs::write(&ini, "[Game Files]\nGameFile0=Base.esm\n").unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        let written = fs::read_to_string(output).unwrap();
        assert!(!written.contains("data="));
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

        run_with(Cli {
            verbose: false,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: vec![data_dir.clone()],
            dry_run: false,
            stdout: false,
            json: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
            positional_ini: None,
            positional_cfg: None,
        })
        .unwrap();

        let written = fs::read_to_string(output).unwrap();
        assert!(written.contains(&format!("data={}\n", data_dir.display())));
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
            "dream-ini-cli-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
