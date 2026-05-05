use std::io::{self, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, Parser};
use clap_complete::Shell;
use dream_ini::{ImportOptions, ImportResult, IniImporter, TextEncoding, serialize_cfg};

const MISSING_INI_EXIT_CODE: u8 = 253;

#[derive(Debug, Parser)]
#[allow(clippy::struct_excessive_bools)]
#[command(
    name = "dream-ini",
    about = "Import Morrowind.ini settings into openmw.cfg",
    version,
    disable_help_flag = true,
    disable_version_flag = true,
    override_usage = "dream-ini --ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]\n       dream-ini --generate-completion <SHELL>\n       dream-ini --generate-manpage",
    after_help = "Import mode requires --ini <FILE>. Optional --cfg <FILE> is read as the base config; without it, import starts empty. Default output is cfg text on stdout with diagnostics on stderr. Use --output <FILE> to write a cfg file, or --in-place with --cfg <FILE> to overwrite the base cfg. Non-import modes (--help, --version, --generate-completion, and --generate-manpage) do not require --ini."
)]
struct Cli {
    /// Verbose output
    #[arg(short, long, display_order = 9)]
    verbose: bool,

    /// Print help
    #[arg(short, long, action = ArgAction::Help, display_order = 6)]
    help: Option<bool>,

    /// Print version
    #[arg(short = 'V', long, action = ArgAction::Version, display_order = 15)]
    version: Option<bool>,

    /// Morrowind.ini file
    #[arg(
        short,
        long,
        value_name = "FILE",
        display_order = 7,
        required_unless_present_any = ["generate_completion", "generate_manpage"]
    )]
    ini: Option<PathBuf>,

    /// openmw.cfg file
    #[arg(short, long, value_name = "FILE", display_order = 1)]
    cfg: Option<PathBuf>,

    /// Output openmw.cfg file
    #[arg(
        short = 'O',
        long,
        value_name = "FILE",
        display_order = 14,
        conflicts_with_all = ["in_place"]
    )]
    output: Option<PathBuf>,

    /// Data Files directory to search before cfg/default data paths
    #[arg(short = 'd', long = "data", value_name = "DIR", display_order = 2)]
    data_dirs: Vec<PathBuf>,

    /// Write the imported result back to the --cfg file
    #[arg(
        short = 'I',
        long,
        display_order = 11,
        requires = "cfg",
        conflicts_with_all = ["output"]
    )]
    in_place: bool,

    /// Generate shell completion script to stdout
    #[arg(
        short = 'C',
        long,
        value_name = "SHELL",
        display_order = 10,
        conflicts_with_all = [
            "generate_manpage",
            "ini",
            "cfg",
            "output",
            "data_dirs",
            "in_place",
            "game_files",
            "fonts",
            "no_archives",
            "encoding",
            "verbose"
        ]
    )]
    generate_completion: Option<Shell>,

    /// Generate roff manpage to stdout
    #[arg(
        short = 'M',
        long,
        display_order = 13,
        conflicts_with_all = [
            "generate_completion",
            "ini",
            "cfg",
            "output",
            "data_dirs",
            "in_place",
            "game_files",
            "fonts",
            "no_archives",
            "encoding",
            "verbose"
        ]
    )]
    generate_manpage: bool,

    /// Import esm and esp files
    #[arg(short = 'g', long = "game-files", display_order = 5)]
    game_files: bool,

    /// Import bitmap fonts
    #[arg(short, long, display_order = 4)]
    fonts: bool,

    /// Disable bsa archives import
    #[arg(short = 'n', long = "no-archives", display_order = 8)]
    no_archives: bool,

    /// Character encoding for imported content-file names: win1250, win1251, or win1252
    #[arg(short, long, value_name = "ENCODING", display_order = 3)]
    encoding: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::MissingIni) => {
            eprintln!("ini file does not exist");
            ExitCode::from(MISSING_INI_EXIT_CODE)
        }
        Err(CliError::InvalidUsage(error)) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
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
    InvalidUsage(String),
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
    validate_import_usage(&cli)?;
    let stdout_mode = !cli.in_place && cli.output.is_none();
    let ini_path = cli.ini.expect("validated --ini");
    let cfg_path = cli.cfg;
    let output_path = cli
        .output
        .clone()
        .or_else(|| cli.in_place.then(|| cfg_path.clone()).flatten());

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
        OutputMode { output_path },
        stdout,
        stderr,
    )?;

    Ok(())
}

fn validate_import_usage(cli: &Cli) -> Result<(), CliError> {
    if cli.ini.is_none() {
        return Err(CliError::InvalidUsage(
            "--ini <FILE> is required for imports".to_owned(),
        ));
    }

    let output_modes = [cli.output.is_some(), cli.in_place]
        .into_iter()
        .filter(|selected| *selected)
        .count();
    if output_modes > 1 {
        return Err(CliError::InvalidUsage(
            "--output and --in-place are mutually exclusive".to_owned(),
        ));
    }

    if cli.in_place && cli.cfg.is_none() {
        return Err(CliError::InvalidUsage(
            "--in-place requires --cfg <FILE>".to_owned(),
        ));
    }

    Ok(())
}

fn handle_generated_output(cli: &Cli, stdout: &mut dyn Write) -> Result<bool, CliError> {
    if let Some(shell) = cli.generate_completion {
        clap_complete::generate(shell, &mut Cli::command(), "dream-ini", stdout);
        return Ok(true);
    }

    if cli.generate_manpage {
        render_manpage(stdout)?;
        return Ok(true);
    }

    Ok(false)
}

fn render_manpage(stdout: &mut dyn Write) -> Result<(), CliError> {
    let manpage = clap_mangen::Man::new(Cli::command());
    manpage.render_title(stdout)?;
    manpage.render_name_section(stdout)?;
    write!(
        stdout,
        ".SH SYNOPSIS\n.B dream-ini\n--ini <FILE> [--cfg <FILE>] [--output <FILE>|--in-place] [options]\n.br\n.B dream-ini\n--generate-completion <SHELL>\n.br\n.B dream-ini\n--generate-manpage\n"
    )?;
    manpage.render_description_section(stdout)?;
    manpage.render_options_section(stdout)?;
    manpage.render_extra_section(stdout)?;
    manpage.render_version_section(stdout)?;
    Ok(())
}

#[derive(Debug)]
struct OutputMode {
    output_path: Option<PathBuf>,
}

fn write_result_output(
    importer: &IniImporter,
    result: &ImportResult,
    mode: OutputMode,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> Result<(), CliError> {
    if let Some(output_path) = mode.output_path {
        diagnostic(
            false,
            stdout,
            stderr,
            format_args!("write to: {}", output_path.display()),
        )?;
        importer.save_config_output(&output_path, &result.cfg)?;
    } else {
        write!(stdout, "{}", serialize_cfg(&result.cfg))?;
    }

    Ok(())
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
    fn accepts_flag_ini_and_cfg() {
        let cli = Cli::parse_from(["dream-ini", "--ini", "mw.ini", "--cfg", "openmw.cfg"]);

        assert_eq!(cli.ini, Some(PathBuf::from("mw.ini")));
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
    }

    #[test]
    fn rejects_positional_paths() {
        let error = Cli::try_parse_from(["dream-ini", "Morrowind.ini", "openmw.cfg"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn clap_requires_ini_for_import_options() {
        let error = Cli::try_parse_from(["dream-ini", "--output", "openmw.cfg"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn parses_import_options() {
        let cli = Cli::parse_from([
            "dream-ini",
            "--game-files",
            "--fonts",
            "-n",
            "--encoding",
            "win1251",
            "--data",
            "Data Files",
            "-d",
            "Alt Data",
            "--output",
            "out.cfg",
            "--ini",
            "mw.ini",
            "--cfg",
            "openmw.cfg",
        ]);

        assert!(cli.game_files);
        assert!(cli.fonts);
        assert!(cli.no_archives);
        assert!(!cli.in_place);
        assert_eq!(
            cli.data_dirs,
            vec![PathBuf::from("Data Files"), PathBuf::from("Alt Data")]
        );
        assert_eq!(cli.encoding.as_deref(), Some("win1251"));
        assert_eq!(cli.output, Some(PathBuf::from("out.cfg")));
    }

    #[test]
    fn parses_in_place_output_mode() {
        let cli = Cli::parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "--in-place",
        ]);

        assert!(cli.in_place);
        assert_eq!(cli.cfg, Some(PathBuf::from("openmw.cfg")));
        assert_eq!(cli.output, None);
    }

    #[test]
    fn parses_short_output_modes() {
        let output = Cli::parse_from(["dream-ini", "--ini", "Morrowind.ini", "-O", "out.cfg"]);
        let in_place = Cli::parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "-I",
        ]);

        assert_eq!(output.output, Some(PathBuf::from("out.cfg")));
        assert!(in_place.in_place);
    }

    #[test]
    fn in_place_requires_cfg_at_parse_time() {
        let error =
            Cli::try_parse_from(["dream-ini", "--ini", "Morrowind.ini", "--in-place"]).unwrap_err();

        assert_eq!(
            error.kind(),
            clap::error::ErrorKind::MissingRequiredArgument
        );
    }

    #[test]
    fn in_place_conflicts_with_other_output_modes_at_parse_time() {
        let output_error = Cli::try_parse_from([
            "dream-ini",
            "--ini",
            "Morrowind.ini",
            "--cfg",
            "openmw.cfg",
            "--in-place",
            "--output",
            "out.cfg",
        ])
        .unwrap_err();
        assert_eq!(
            output_error.kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }

    #[test]
    fn help_mentions_core_options() {
        let help = Cli::command().render_help().to_string();

        assert!(help.contains("--game-files"));
        assert!(help.contains("--fonts"));
        assert!(help.contains("--no-archives"));
        assert!(help.contains("--encoding"));
        assert!(help.contains("--output"));
        assert!(help.contains("--data"));
        assert!(help.contains("--in-place"));
        assert!(help.contains("--generate-completion"));
        assert!(help.contains("--generate-manpage"));

        let ordered_options = [
            "-c, --cfg",
            "-d, --data",
            "-e, --encoding",
            "-f, --fonts",
            "-g, --game-files",
            "-h, --help",
            "-i, --ini",
            "-n, --no-archives",
            "-v, --verbose",
            "-C, --generate-completion",
            "-I, --in-place",
            "-M, --generate-manpage",
            "-O, --output",
            "-V, --version",
        ];
        let mut previous_position = 0;
        for option in ordered_options {
            let position = help
                .find(option)
                .unwrap_or_else(|| panic!("missing {option}"));
            assert!(position >= previous_position, "{option} is out of order");
            previous_position = position;
        }
    }

    #[test]
    fn rejects_old_no_archives_short_flag() {
        let error = Cli::try_parse_from(["dream-ini", "--ini", "Morrowind.ini", "-A"]).unwrap_err();

        assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
    }

    #[test]
    fn parses_generation_options() {
        let completion = Cli::parse_from(["dream-ini", "--generate-completion", "bash"]);
        assert_eq!(completion.generate_completion, Some(Shell::Bash));

        let manpage = Cli::parse_from(["dream-ini", "--generate-manpage"]);
        assert!(manpage.generate_manpage);

        let short_completion = Cli::parse_from(["dream-ini", "-C", "bash"]);
        assert_eq!(short_completion.generate_completion, Some(Shell::Bash));

        let short_manpage = Cli::parse_from(["dream-ini", "-M"]);
        assert!(short_manpage.generate_manpage);
    }

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
                data_dirs: Vec::new(),
                in_place: false,
                generate_completion: Some(Shell::Bash),
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: false,
                encoding: None,
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
                data_dirs: Vec::new(),
                in_place: false,
                generate_completion: None,
                generate_manpage: true,
                game_files: false,
                fonts: false,
                no_archives: false,
                encoding: None,
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
            help: None,
            version: None,
            ini: Some(ini),
            cfg: Some(cfg),
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
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
            help: None,
            version: None,
            ini: Some(ini),
            cfg: Some(cfg.clone()),
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
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
            help: None,
            version: None,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
        })
        .unwrap();

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
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_writers(
            Cli {
                verbose: false,
                help: None,
                version: None,
                ini: Some(ini),
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                in_place: false,
                generate_completion: None,
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: true,
                encoding: None,
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
    fn run_with_cfg_without_in_place_previews_to_stdout() {
        let dir = unique_test_dir("cfg-preview-run");
        fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("Morrowind.ini");
        let cfg = dir.join("openmw.cfg");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
        fs::write(&cfg, "encoding=win1252\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_writers(
            Cli {
                verbose: false,
                help: None,
                version: None,
                ini: Some(ini),
                cfg: Some(cfg.clone()),
                output: None,
                data_dirs: Vec::new(),
                in_place: false,
                generate_completion: None,
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: true,
                encoding: None,
            },
            &mut stdout,
            &mut stderr,
        )
        .unwrap();

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
            data_dirs: Vec::new(),
            in_place: true,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
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

        run_with(Cli {
            verbose: false,
            help: None,
            version: None,
            ini: Some(ini),
            cfg: Some(cfg.clone()),
            output: None,
            data_dirs: Vec::new(),
            in_place: true,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: true,
            encoding: None,
        })
        .unwrap();

        assert!(fs::read_to_string(cfg).unwrap().contains("no-sound=1"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn run_without_ini_returns_usage_error() {
        let error = run_with(Cli {
            verbose: false,
            help: None,
            version: None,
            ini: None,
            cfg: None,
            output: Some(PathBuf::from("out.cfg")),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
        })
        .unwrap_err();

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
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();

        run_with_writers(
            Cli {
                verbose: false,
                help: None,
                version: None,
                ini: Some(ini),
                cfg: None,
                output: None,
                data_dirs: Vec::new(),
                in_place: false,
                generate_completion: None,
                generate_manpage: false,
                game_files: false,
                fonts: false,
                no_archives: false,
                encoding: None,
            },
            &mut stdout,
            &mut stderr,
        )
        .unwrap();

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
            data_dirs: Vec::new(),
            in_place: true,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: None,
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
            help: None,
            version: None,
            ini: Some(ini),
            cfg: Some(cfg),
            output: None,
            data_dirs: Vec::new(),
            in_place: true,
            generate_completion: None,
            generate_manpage: false,
            game_files: false,
            fonts: false,
            no_archives: false,
            encoding: Some("bogus".to_owned()),
        })
        .unwrap_err();

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

        run_with(Cli {
            verbose: true,
            help: None,
            version: None,
            ini: Some(ini),
            cfg: Some(cfg),
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
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
            help: None,
            version: None,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: Vec::new(),
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
        })
        .unwrap();

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

        run_with(Cli {
            verbose: false,
            help: None,
            version: None,
            ini: Some(ini),
            cfg: None,
            output: Some(output.clone()),
            data_dirs: vec![data_dir.clone()],
            in_place: false,
            generate_completion: None,
            generate_manpage: false,
            game_files: true,
            fonts: false,
            no_archives: true,
            encoding: None,
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
