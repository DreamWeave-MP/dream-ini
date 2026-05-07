use std::fmt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportEvent {
    ContentFileResolved { path: PathBuf, modified: SystemTime },
    ArchiveResolved { path: PathBuf },
    DataDirAddedForContent { path: PathBuf },
    DataDirAddedForArchive { path: PathBuf },
}

impl fmt::Display for ImportEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContentFileResolved { path, modified } => write!(
                f,
                "content file: {} timestamp = ({})",
                path.display(),
                system_time_seconds(*modified)
            ),
            Self::DataDirAddedForContent { path } => write!(
                f,
                "adding data directory used to resolve content files: {}",
                path.display()
            ),
            Self::ArchiveResolved { path } => write!(f, "archive: {}", path.display()),
            Self::DataDirAddedForArchive { path } => write!(
                f,
                "adding data directory used to resolve fallback archives: {}",
                path.display()
            ),
        }
    }
}

fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
