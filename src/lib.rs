//! Library support for importing Morrowind INI settings into OpenMW-style configuration data.
//!
//! The crate exposes the same core importer used by the `dream-ini` CLI. Configuration data is
//! represented as a multimap (`key -> Vec<value>`) so duplicate cfg keys such as `data`, `content`,
//! and `fallback` are preserved without special cases.
//! Path values exposed through cfg text, Lua tables, and import events are UTF-8 strings;
//! non-UTF-8 operating-system paths are outside the supported API contract.
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//!
//! use dream_ini::{ImportOptions, IniImporter};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let importer = IniImporter::new(ImportOptions::default());
//! let result = importer.import_optional_cfg_path(
//!     Path::new("Morrowind.ini"),
//!     Some(Path::new("openmw.cfg")),
//! )?;
//!
//! for warning in &result.warnings {
//!     eprintln!("Warning: {warning}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Enable the `lua` feature to expose an embedding-oriented Lua API via [`lua::create_module`].

use std::collections::BTreeMap;
use std::fmt;
use std::io;
use std::path::PathBuf;

use encoding_rs::{Encoding, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252};

mod content_files;
mod events;
mod fallback_keys;
mod importer;
#[cfg(feature = "lua")]
pub mod lua;
mod parser;
mod plugin;
#[cfg(test)]
mod test_support;
mod warnings;

pub use events::ImportEvent;
pub use importer::{ImportOptions, ImportReport, ImportResult, IniImporter};
pub use parser::{
    ParsedIni, parse_cfg_str, parse_ini_bytes, parse_ini_bytes_with_warnings, parse_ini_str,
    parse_ini_str_with_warnings, serialize_cfg,
};
pub use plugin::{PluginHeader, read_plugin_header};
pub use warnings::ImportWarning;

pub type MultiMap = BTreeMap<String, Vec<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Game {
    Morrowind,
}

impl Game {
    pub(crate) fn plugin_format(self) -> PluginFormat {
        match self {
            Self::Morrowind => PluginFormat::Tes3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginFormat {
    Tes3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextEncoding {
    Win1250,
    Win1251,
    Win1252,
}

impl TextEncoding {
    /// Parses an `OpenMW` encoding label.
    ///
    /// # Errors
    /// Returns [`ImportError::UnsupportedEncoding`] if `value` is not supported.
    pub fn parse(value: &str) -> Result<Self, ImportError> {
        match value.to_ascii_lowercase().as_str() {
            "win1250" | "windows-1250" => Ok(Self::Win1250),
            "win1251" | "windows-1251" => Ok(Self::Win1251),
            "win1252" | "windows-1252" => Ok(Self::Win1252),
            _ => Err(ImportError::UnsupportedEncoding(value.to_owned())),
        }
    }

    pub(crate) fn as_label(self) -> &'static str {
        match self {
            Self::Win1250 => "win1250",
            Self::Win1251 => "win1251",
            Self::Win1252 => "win1252",
        }
    }

    pub(crate) fn encoding_rs(self) -> &'static Encoding {
        match self {
            Self::Win1250 => WINDOWS_1250,
            Self::Win1251 => WINDOWS_1251,
            Self::Win1252 => WINDOWS_1252,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ImportError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    UnsupportedEncoding(String),
    InvalidPluginHeader {
        path: PathBuf,
        message: String,
    },
    MissingContentFiles {
        files: Vec<String>,
        searched_paths: Vec<PathBuf>,
    },
    MissingArchives {
        files: Vec<String>,
        searched_paths: Vec<PathBuf>,
    },
    InvalidContentFileName(String),
    InvalidArchiveName(String),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => write!(f, "{}: {}", path.display(), source),
            Self::UnsupportedEncoding(value) => write!(f, "unsupported encoding: {value}"),
            Self::InvalidPluginHeader { path, message } => {
                write!(f, "invalid plugin header in {}: {message}", path.display())
            }
            Self::MissingContentFiles {
                files,
                searched_paths,
            } => {
                write!(f, "content files not found: {}", files.join(", "))?;
                if !searched_paths.is_empty() {
                    write!(
                        f,
                        "; searched: {}",
                        searched_paths
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                write!(f, "; pass --data or add data=... to the cfg")
            }
            Self::MissingArchives {
                files,
                searched_paths,
            } => {
                write!(f, "fallback archives not found: {}", files.join(", "))?;
                if !searched_paths.is_empty() {
                    write!(
                        f,
                        "; searched: {}",
                        searched_paths
                            .iter()
                            .map(|path| path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                write!(f, "; pass --data or add data=... to the cfg")
            }
            Self::InvalidContentFileName(file) => write!(
                f,
                "invalid content file name: {file}; content entries must be plugin filenames, not paths"
            ),
            Self::InvalidArchiveName(file) => write!(
                f,
                "invalid fallback archive name: {file}; archive entries must be BSA filenames, not paths"
            ),
        }
    }
}

impl std::error::Error for ImportError {}

#[must_use]
pub fn known_fallback_keys() -> &'static [&'static str] {
    fallback_keys::MORROWIND_FALLBACK_KEYS
}

#[cfg(test)]
mod lib_tests;
