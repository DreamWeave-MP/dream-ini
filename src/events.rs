use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ImportEvent {
    ContentFileResolved { path: PathBuf, modified: SystemTime },
    DataDirAddedForContent { path: PathBuf },
}

pub(crate) fn format_import_events(events: &[ImportEvent]) -> Vec<String> {
    events.iter().map(format_import_event).collect()
}

fn format_import_event(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, modified } => {
            format!(
                "content file: {} timestamp = ({})",
                path.display(),
                system_time_seconds(*modified)
            )
        }
        ImportEvent::DataDirAddedForContent { path } => format!(
            "adding data directory used to resolve content files: {}",
            path.display()
        ),
    }
}

fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
