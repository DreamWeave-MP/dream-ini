use std::process::ExitCode;

use command::{CliError, MISSING_INI_EXIT_CODE};

mod cli;
mod command;
mod generated;

fn main() -> ExitCode {
    match command::run() {
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
