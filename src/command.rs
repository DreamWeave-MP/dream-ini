use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use dream_ini::{
    ImportOptions, ImportResult, IniImporter, TextEncoding, save_resolved_cfg_to_path,
    serialize_resolved_cfg,
};

use crate::cli::Cli;
use crate::generated::handle_generated_output;

pub(crate) const MISSING_INI_EXIT_CODE: u8 = 253;

#[derive(Debug)]
pub(crate) enum CliError {
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

pub(crate) fn run() -> Result<(), CliError> {
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
    let cfg_reference_path = output_path.as_deref().or(cfg_path.as_deref());
    let cfg_reference_path = cfg_reference_path.map(Path::to_owned);

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
        data_dirs: cli.data_dir.into_iter().collect(),
        data_local: cli.data_local,
        resources: cli.resources,
        user_data: cli.user_data,
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
    for event in &result.events {
        diagnostic(stdout_mode, stdout, stderr, format_args!("{event}"))?;
    }
    for warning in &result.warnings {
        writeln!(stderr, "Warning: {warning}")?;
    }

    write_result_output(
        &result,
        OutputMode {
            output_path,
            cfg_reference_path,
        },
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

#[derive(Debug)]
struct OutputMode {
    output_path: Option<PathBuf>,
    cfg_reference_path: Option<PathBuf>,
}

fn write_result_output(
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
        save_resolved_cfg_to_path(&result.cfg, &output_path)?;
    } else {
        let user_config_dir = mode
            .cfg_reference_path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new(""));
        write!(
            stdout,
            "{}",
            serialize_resolved_cfg(&result.cfg, user_config_dir)?
        )?;
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
mod tests;
