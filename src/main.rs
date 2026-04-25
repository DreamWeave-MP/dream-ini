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
    override_usage = "rome-ini <options> inifile configfile"
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
    let Some(cfg_path) = cli.cfg.or(cli.positional_cfg) else {
        Cli::command().print_help()?;
        println!();
        return Ok(());
    };

    if !ini_path.exists() {
        return Err(CliError::MissingIni);
    }

    if !cfg_path.exists() {
        eprintln!("cfg file does not exist; starting from an empty config");
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

    let output_path = cli.output.unwrap_or_else(|| cfg_path.clone());
    let importer = IniImporter::new(options);

    println!("load cfg file: {}", cfg_path.display());
    println!("load ini file: {}", ini_path.display());

    let result = importer.import_config_paths(&ini_path, &cfg_path)?;
    for warning in &result.imported.warnings {
        eprintln!("Warning: {warning}");
    }

    println!("write to: {}", output_path.display());
    importer.save_config_output(&output_path, &result.output_cfg)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
