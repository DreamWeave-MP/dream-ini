// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

const LOG_FILE_NAME: &str = "dream-ini-portmaster.log";

pub(super) type SharedLog = Arc<Mutex<File>>;

pub(super) fn open_log() -> Option<File> {
    let paths = log_paths();
    for path in paths {
        match OpenOptions::new().create(true).append(true).open(&path) {
            Ok(file) => return Some(file),
            Err(error) => eprintln!(
                "failed to open PortMaster log at {}: {error}",
                path.display()
            ),
        }
    }
    None
}

fn log_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = env::current_exe()
        && let Some(parent) = exe.parent()
    {
        paths.push(parent.join(LOG_FILE_NAME));
    }
    if let Ok(cwd) = env::current_dir() {
        paths.push(cwd.join(LOG_FILE_NAME));
    }
    paths.push(PathBuf::from(LOG_FILE_NAME));
    paths
}

pub(super) fn install_panic_hook(log: Option<SharedLog>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        write_log(log.as_ref(), format!("panic: {panic_info}"));
        previous(panic_info);
    }));
}

pub(super) fn log_startup(log: Option<&SharedLog>) {
    write_log(log, "startup compile_feature=portmaster-gui");
    write_log(
        log,
        format!("argv={:?}", env::args_os().collect::<Vec<_>>()),
    );
    write_log(log, format!("cwd={:?}", env::current_dir()));
    write_log(log, format!("current_exe={:?}", env::current_exe()));
    write_log(log, format!("unix_timestamp={}", unix_timestamp()));
}

pub(super) fn write_log(log: Option<&SharedLog>, message: impl AsRef<str>) {
    let Some(log) = log else {
        return;
    };
    if let Ok(mut file) = log.lock() {
        let _ = writeln!(file, "{} {}", unix_timestamp(), message.as_ref());
        let _ = file.flush();
    }
}

fn unix_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}
