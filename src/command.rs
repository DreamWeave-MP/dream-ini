use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use dream_ini::{
    ImportError, ImportOptions, ImportResult, IniImporter, MultiMap, TextEncoding, serialize_cfg,
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

    if !ini_path.exists() {
        return Err(CliError::MissingIni);
    }
    validate_resources_path(cli.resources.as_deref(), cfg_reference_path)?;

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
        userdata: cli.userdata,
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

    write_result_output(&result, OutputMode { output_path }, stdout, stderr)?;

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

fn validate_resources_path(
    resources: Option<&Path>,
    cfg_reference_path: Option<&Path>,
) -> Result<(), CliError> {
    let Some(resources) = resources else {
        return Ok(());
    };
    let resolved = resolve_cfg_relative_path(resources, cfg_reference_path);
    let metadata = fs::metadata(&resolved).map_err(|source| {
        CliError::InvalidUsage(format!(
            "--resources must resolve to an installed, non-empty directory: {} ({source})",
            resources.display()
        ))
    })?;
    if !metadata.is_dir() {
        return Err(CliError::InvalidUsage(format!(
            "--resources must be a directory, not a file: {}",
            resources.display()
        )));
    }
    let mut entries = fs::read_dir(&resolved).map_err(|source| {
        CliError::InvalidUsage(format!(
            "--resources directory cannot be read: {} ({source})",
            resources.display()
        ))
    })?;
    if entries
        .next()
        .transpose()
        .map_err(|source| {
            CliError::InvalidUsage(format!(
                "--resources directory cannot be read: {} ({source})",
                resources.display()
            ))
        })?
        .is_none()
    {
        return Err(CliError::InvalidUsage(format!(
            "--resources must not be an empty directory: {}",
            resources.display()
        )));
    }
    Ok(())
}

fn resolve_cfg_relative_path(path: &Path, cfg_reference_path: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        return path.to_owned();
    }
    cfg_reference_path
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new(""))
        .join(path)
}

#[derive(Debug)]
struct OutputMode {
    output_path: Option<PathBuf>,
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
        save_config_output(&output_path, &result.cfg)?;
    } else {
        write!(stdout, "{}", serialize_cfg(&result.cfg))?;
    }

    Ok(())
}

fn save_config_output(output_path: &Path, cfg: &MultiMap) -> Result<(), ImportError> {
    write_atomic(output_path, serialize_cfg(cfg).as_bytes())
}

fn write_atomic(output_path: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    let temp_path = temporary_output_path(output_path);
    let result = write_atomic_inner(output_path, &temp_path, bytes);
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn write_atomic_inner(
    output_path: &Path,
    temp_path: &Path,
    bytes: &[u8],
) -> Result<(), ImportError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp_path)
        .map_err(|source| io_error(temp_path, source))?;
    file.write_all(bytes)
        .map_err(|source| io_error(temp_path, source))?;
    file.sync_all()
        .map_err(|source| io_error(temp_path, source))?;
    drop(file);

    fs::rename(temp_path, output_path).map_err(|source| io_error(output_path, source))
}

fn temporary_output_path(output_path: &Path) -> PathBuf {
    let file_name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("openmw.cfg");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temp_name = format!(".{file_name}.dream-ini-{}-{unique}.tmp", std::process::id());
    output_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(temp_name)
}

fn io_error(path: &Path, source: io::Error) -> ImportError {
    ImportError::Io {
        path: path.to_owned(),
        source,
    }
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
