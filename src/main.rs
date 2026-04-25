use std::path::PathBuf;
use std::process::ExitCode;

use clap::{CommandFactory, Parser};
use rome_ini::{ImportOptions, IniImporter, TextEncoding};

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
        Err(error) => {
            eprintln!("ERROR: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
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
        return Err("ini file does not exist".into());
    }

    if !cfg_path.exists() {
        eprintln!("cfg file does not exist; starting from an empty config");
        if let Some(parent) = cfg_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&cfg_path, "")?;
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

    if cli.verbose {
        println!("load cfg file: {}", cfg_path.display());
        println!("load ini file: {}", ini_path.display());
    }

    let result = importer.import_config_paths(&ini_path, &cfg_path)?;
    for warning in &result.imported.warnings {
        eprintln!("Warning: {warning}");
    }

    if cli.verbose {
        println!("write to: {}", output_path.display());
    }
    importer.save_config_output(&output_path, &result.config)?;

    Ok(())
}
