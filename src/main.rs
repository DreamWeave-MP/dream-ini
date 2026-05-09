use std::process::ExitCode;

use command::{CliError, MISSING_INI_EXIT_CODE};

mod cli;
mod command;
mod desktop_entry;
mod generated;
#[cfg(feature = "gui")]
mod gui;

fn main() -> ExitCode {
    #[cfg(feature = "gui")]
    if std::env::args_os().len() == 1 {
        return gui::run();
    }

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
