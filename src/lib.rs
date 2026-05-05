//! Library support for importing Morrowind INI settings into OpenMW-style configuration data.
//!
//! The crate exposes the same core importer used by the `dream-ini` CLI. Configuration data is
//! represented as a multimap (`key -> Vec<value>`) so duplicate cfg keys such as `data`, `content`,
//! and `fallback` are preserved without special cases.
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

mod fallback_keys;
mod importer;
#[cfg(feature = "lua")]
pub mod lua;
mod parser;
mod plugin;

pub use importer::{ImportOptions, ImportReport, ImportResult, IniImporter};
pub use parser::{
    ParsedIni, parse_cfg_str, parse_ini_bytes, parse_ini_bytes_with_warnings, parse_ini_str,
    parse_ini_str_with_warnings, serialize_cfg,
};
pub use plugin::{PluginHeader, read_plugin_header};

#[cfg(test)]
use importer::import_archives;
#[cfg(test)]
use plugin::{apply_morrowind_expansion_order, dependency_sort};

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
    InvalidContentFileName(String),
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
            Self::InvalidContentFileName(file) => write!(
                f,
                "invalid content file name: {file}; content entries must be plugin filenames, not paths"
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
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn values<'a>(map: &'a MultiMap, key: &str) -> &'a [String] {
        map.get(key).map_or(&[], Vec::as_slice)
    }

    #[test]
    fn parses_ini_sections_comments_duplicates_and_equals() {
        let parsed = parse_ini_str(
            "[General]\nDisable Audio=1 ; comment\nName=a=b\nName=c\n=ignored\nEmpty=\n[bad\nignored\n",
        );

        assert_eq!(values(&parsed, "General:Disable Audio"), &["1 ".to_owned()]);
        assert_eq!(
            values(&parsed, "General:Name"),
            &["a=b".to_owned(), "c".to_owned()]
        );
        assert!(!parsed.contains_key("General:Empty"));
    }

    #[test]
    fn surfaces_ini_parse_warnings() {
        let parsed = parse_ini_str_with_warnings("[General]\nEmpty=\n[bad\n[]=ignored\n");

        assert_eq!(
            parsed.warnings,
            vec![
                "ignored empty value for key 'General:Empty'.".to_owned(),
                "ini file wrongly formatted ([bad). Line ignored.".to_owned(),
                "ini file wrongly formatted ([]=ignored). Line ignored.".to_owned(),
            ]
        );
    }

    #[test]
    fn parses_ini_keys_before_section_like_cpp_importer() {
        let parsed = parse_ini_str("Loose=value\n");
        assert_eq!(values(&parsed, ":Loose"), &["value".to_owned()]);
    }

    #[test]
    fn parses_cfg_trims_and_preserves_inline_hash() {
        let parsed = parse_cfg_str(" # comment\nkey = value # not comment\nkey= second\ninvalid\n");
        assert_eq!(
            values(&parsed, "key"),
            &["value # not comment".to_owned(), "second".to_owned()]
        );
    }

    #[test]
    fn decodes_ini_with_selected_codepage() {
        let parsed = parse_ini_bytes(b"[Movies]\nNew Game=caf\xe9.bik\n", TextEncoding::Win1252);
        assert_eq!(
            values(&parsed, "Movies:New Game"),
            &["caf\u{e9}.bik".to_owned()]
        );
    }

    #[test]
    fn imports_merge_fallback_and_archives() {
        let importer = IniImporter::new(ImportOptions::default());
        let mut cfg = parse_cfg_str("no-sound=0\nfallback=old\n");
        let ini = parse_ini_str(
            "[General]\nDisable Audio=1\nDisable Audio=0\n[Fonts]\nFont 0=magic\n[Archives]\nArchive 0=Tribunal.bsa\nArchive 1=Bloodmoon.bsa\n[Movies]\nNew Game=intro.bik\n",
        );

        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "no-sound"), &["0".to_owned()]);
        assert_eq!(
            values(&cfg, "fallback-archive"),
            &[
                "Morrowind.bsa".to_owned(),
                "Tribunal.bsa".to_owned(),
                "Bloodmoon.bsa".to_owned()
            ]
        );
        assert_eq!(
            values(&cfg, "fallback"),
            &["Movies_New_Game,intro.bik".to_owned()]
        );
        assert!(result.warnings.is_empty());
        assert!(result.messages.is_empty());
    }

    #[test]
    fn font_import_is_option_gated() {
        let ini = parse_ini_str("[Fonts]\nFont 0=magic\n[Movies]\nNew Game=intro.bik\n");
        let mut cfg = MultiMap::new();
        let importer = IniImporter::new(ImportOptions::default());
        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();
        assert_eq!(
            values(&cfg, "fallback"),
            &["Movies_New_Game,intro.bik".to_owned()]
        );
        assert!(result.messages.is_empty());

        let mut cfg = MultiMap::new();
        let importer = IniImporter::new(ImportOptions {
            import_fonts: true,
            ..ImportOptions::default()
        });
        let result = importer
            .import_maps(&mut cfg, &ini, Path::new("Morrowind.ini"))
            .unwrap();
        assert_eq!(
            values(&cfg, "fallback"),
            &[
                "Fonts_Font_0,magic".to_owned(),
                "Movies_New_Game,intro.bik".to_owned()
            ]
        );
        assert!(result.messages.is_empty());
    }

    #[test]
    fn archive_import_stops_at_first_missing_index() {
        let ini = parse_ini_str("[Archives]\nArchive 0=First.bsa\nArchive 2=Skipped.bsa\n");
        let mut cfg = MultiMap::new();
        import_archives(&mut cfg, &ini);
        assert_eq!(
            values(&cfg, "fallback-archive"),
            &["Morrowind.bsa".to_owned(), "First.bsa".to_owned()]
        );
    }

    #[test]
    fn dependency_sort_places_masters_before_dependents() {
        let sorted = dependency_sort(vec![
            ("Patch.esp".to_owned(), vec!["Base.esm".to_owned()]),
            ("Base.esm".to_owned(), vec![]),
        ]);
        assert_eq!(sorted, vec!["Base.esm".to_owned(), "Patch.esp".to_owned()]);
    }

    #[test]
    fn applies_morrowind_expansion_order() {
        let mut files = vec![
            "Morrowind.esm".to_owned(),
            "Bloodmoon.esm".to_owned(),
            "Tribunal.esm".to_owned(),
        ];
        apply_morrowind_expansion_order(&mut files);
        assert_eq!(
            files,
            vec!["Morrowind.esm", "Tribunal.esm", "Bloodmoon.esm"]
        );
    }

    #[test]
    fn reads_tes3_header_masters() {
        let dir = unique_test_dir("tes3-header");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Patch.esp");
        fs::write(&plugin, tes3_bytes(&["Morrowind.esm", "Tribunal.esm"])).unwrap();

        let header =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

        assert_eq!(header.name, "Patch.esp");
        assert_eq!(header.masters, vec!["Morrowind.esm", "Tribunal.esm"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reads_tes3_header_masters_with_selected_encoding() {
        let dir = unique_test_dir("tes3-header-encoding");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Patch.esp");
        fs::write(&plugin, tes3_bytes_from_master_bytes(&[b"caf\xe9.esm"])).unwrap();

        let header =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap();

        assert_eq!(header.masters, vec!["caf\u{e9}.esm"]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_invalid_tes3_header() {
        let dir = unique_test_dir("tes3-invalid");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Bad.esp");
        fs::write(&plugin, b"TES4").unwrap();

        let error =
            read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252).unwrap_err();
        assert!(matches!(error, ImportError::InvalidPluginHeader { .. }));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_truncated_tes3_record() {
        let dir = unique_test_dir("tes3-truncated-record");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Bad.esp");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&8u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        fs::write(&plugin, bytes).unwrap();

        let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
            .unwrap_err()
            .to_string();
        assert!(error.contains("TES3 record extends past end of file"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_truncated_tes3_subrecord() {
        let dir = unique_test_dir("tes3-truncated-subrecord");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Bad.esp");
        let mut record = Vec::new();
        record.extend_from_slice(b"MAST");
        record.extend_from_slice(&8u32.to_le_bytes());
        record.extend_from_slice(b"short");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        fs::write(&plugin, bytes).unwrap();

        let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
            .unwrap_err()
            .to_string();
        assert!(error.contains("subrecord extends past TES3 record"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_truncated_non_master_tes3_subrecord_data() {
        let dir = unique_test_dir("tes3-truncated-non-master-data");
        fs::create_dir_all(&dir).unwrap();
        let plugin = dir.join("Bad.esp");
        let mut record = Vec::new();
        record.extend_from_slice(b"HEDR");
        record.extend_from_slice(&300u32.to_le_bytes());
        record.extend_from_slice(&[0; 8]);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&308u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        fs::write(&plugin, bytes).unwrap();

        let error = read_plugin_header(&plugin, PluginFormat::Tes3, TextEncoding::Win1252)
            .unwrap_err()
            .to_string();
        assert!(error.contains("TES3 record extends past end of file"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_game_files_using_tes3_dependencies() {
        let dir = unique_test_dir("game-files");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
        fs::write(data_dir.join("Patch.esp"), tes3_bytes(&["Base.esm"])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Patch.esp\nGameFile1=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let result = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(
            values(&cfg, "content"),
            &["Base.esm".to_owned(), "Patch.esp".to_owned()]
        );
        assert!(result.messages.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn verbose_game_file_import_reports_content_file_messages() {
        let dir = unique_test_dir("game-files-verbose");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            verbose: true,
            ..ImportOptions::default()
        });

        let result = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(result.messages.len(), 1);
        assert!(result.messages[0].contains("content file:"));
        assert!(result.messages[0].contains("Base.esm"));
        assert!(result.messages[0].contains("timestamp = ("));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_game_files_from_default_data_files_path_and_writes_data() {
        let dir = unique_test_dir("game-files-default-data");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let result = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        assert_eq!(
            values(&cfg, "data"),
            &[data_dir.to_string_lossy().into_owned()]
        );
        assert_eq!(result.messages.len(), 1);
        assert!(result.messages[0].contains("adding data directory"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn explicit_data_dir_is_written_when_it_resolves_content() {
        let dir = unique_test_dir("game-files-explicit-data");
        let data_dir = dir.join("External Data");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            data_dirs: vec![data_dir.clone()],
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        assert_eq!(
            values(&cfg, "data"),
            &[data_dir.to_string_lossy().into_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repeated_explicit_data_dirs_use_search_order() {
        let dir = unique_test_dir("game-files-explicit-data-order");
        let first_data_dir = dir.join("First Data");
        let second_data_dir = dir.join("Second Data");
        fs::create_dir_all(&first_data_dir).unwrap();
        fs::create_dir_all(&second_data_dir).unwrap();
        fs::write(second_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            data_dirs: vec![first_data_dir, second_data_dir.clone()],
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        assert_eq!(
            values(&cfg, "data"),
            &[second_data_dir.to_string_lossy().into_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_data_files_path_is_not_duplicated() {
        let dir = unique_test_dir("game-files-default-data-duplicate");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        assert_eq!(
            values(&cfg, "data"),
            &[data_dir.to_string_lossy().into_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn default_data_files_path_is_not_added_when_config_data_resolves_content() {
        let dir = unique_test_dir("game-files-config-data-wins");
        let default_data_dir = dir.join("Data Files");
        let configured_data_dir = dir.join("Configured Data");
        fs::create_dir_all(&default_data_dir).unwrap();
        fs::create_dir_all(&configured_data_dir).unwrap();
        fs::write(default_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
        fs::write(configured_data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", configured_data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        assert_eq!(
            values(&cfg, "data"),
            &[configured_data_dir.to_string_lossy().into_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_default_data_files_path_fails_import() {
        let dir = unique_test_dir("game-files-default-data-missing");
        fs::create_dir_all(&dir).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Missing.esm\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let error = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap_err()
            .to_string();

        assert_eq!(values(&cfg, "content"), &[] as &[String]);
        assert_eq!(values(&cfg, "data"), &[] as &[String]);
        assert!(error.contains("content files not found: Missing.esm"));
        assert!(error.contains("pass --data or add data=..."));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn missing_game_files_fail_import() {
        let dir = unique_test_dir("game-files-missing");
        fs::create_dir_all(&dir).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Missing.esp\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let error = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap_err()
            .to_string();

        assert_eq!(values(&cfg, "content"), &[] as &[String]);
        assert!(error.contains("content files not found: Missing.esp"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn partially_resolved_game_files_fail_without_writing_partial_content() {
        let dir = unique_test_dir("game-files-partial");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\nGameFile1=Missing.esp\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let error = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap_err()
            .to_string();

        assert_eq!(values(&cfg, "content"), &[] as &[String]);
        assert_eq!(values(&cfg, "data"), &[] as &[String]);
        assert!(error.contains("content files not found: Missing.esp"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sparse_game_file_indices_are_imported() {
        let dir = unique_test_dir("game-files-sparse");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();
        fs::write(data_dir.join("Patch.esp"), tes3_bytes(&["Base.esm"])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm\nGameFile2=Patch.esp\n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(
            values(&cfg, "content"),
            &["Base.esm".to_owned(), "Patch.esp".to_owned()]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn game_file_values_are_trimmed_before_resolution() {
        let dir = unique_test_dir("game-files-trimmed");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = parse_cfg_str(&format!("data={}\n", data_dir.display()));
        let ini = parse_ini_str("[Game Files]\nGameFile0=Base.esm \n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap();

        assert_eq!(values(&cfg, "content"), &["Base.esm".to_owned()]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn content_file_paths_are_rejected() {
        let dir = unique_test_dir("game-files-path-entry");
        let data_dir = dir.join("Data Files");
        fs::create_dir_all(&data_dir).unwrap();
        fs::write(data_dir.join("Base.esm"), tes3_bytes(&[])).unwrap();

        let mut cfg = MultiMap::new();
        let ini = parse_ini_str("[Game Files]\nGameFile0=../Data Files/Base.esm \n");
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        let error = importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid content file name: ../Data Files/Base.esm"));
        assert_eq!(values(&cfg, "content"), &[] as &[String]);
        assert_eq!(values(&cfg, "data"), &[] as &[String]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn failed_game_file_import_leaves_cfg_unchanged() {
        let dir = unique_test_dir("game-files-error-atomic");
        fs::create_dir_all(&dir).unwrap();

        let mut cfg = parse_cfg_str("fallback=Old_Setting,old\nno-sound=0\n");
        let original_cfg = cfg.clone();
        let ini = parse_ini_str(
            "[General]\nDisable Audio=1\n[Weather]\nSunrise Time=6\n[Game Files]\nGameFile0=Missing.esm\n",
        );
        let importer = IniImporter::new(ImportOptions {
            import_game_files: true,
            import_archives: false,
            ..ImportOptions::default()
        });

        importer
            .import_maps(&mut cfg, &ini, &dir.join("Morrowind.ini"))
            .unwrap_err();

        assert_eq!(cfg, original_cfg);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_paths_preserves_existing_cfg_and_writes_imports() {
        let dir = unique_test_dir("path-import");
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("openmw.cfg");
        let ini = dir.join("Morrowind.ini");
        let output = dir.join("imported.cfg");
        fs::write(
            &cfg,
            "no-sound=0\nfallback=Old_Setting,old\nencoding=win1252\n",
        )
        .unwrap();
        fs::write(
            &ini,
            "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n[Archives]\nArchive 0=Tribunal.bsa\n",
        )
        .unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_paths(&ini, &cfg).unwrap();

        assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
        assert_eq!(values(&result.cfg, "encoding"), &["win1252".to_owned()]);
        assert_eq!(
            values(&result.cfg, "fallback"),
            &["Movies_New_Game,intro.bik".to_owned()]
        );
        assert_eq!(
            values(&result.cfg, "fallback-archive"),
            &["Morrowind.bsa".to_owned(), "Tribunal.bsa".to_owned()]
        );

        importer.save_config_output(&output, &result.cfg).unwrap();
        let written = fs::read_to_string(&output).unwrap();
        assert!(written.contains("no-sound=1"));
        assert!(written.contains("fallback=Movies_New_Game,intro.bik"));
        assert!(written.contains("fallback-archive=Morrowind.bsa"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_paths_writes_exact_golden_output() {
        let dir = unique_test_dir("golden-output");
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("openmw.cfg");
        let ini = dir.join("Morrowind.ini");
        fs::write(
            &cfg,
            "resources=resources\nno-sound=0\nfallback=Old_Setting,old\n",
        )
        .unwrap();
        fs::write(
            &ini,
            "[General]\nDisable Audio=1\n[Movies]\nNew Game=intro.bik\n[Archives]\nArchive 0=Tribunal.bsa\n",
        )
        .unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_paths(&ini, &cfg).unwrap();

        assert_eq!(
            serialize_cfg(&result.cfg),
            concat!(
                "encoding=win1252\n",
                "fallback=Movies_New_Game,intro.bik\n",
                "fallback-archive=Morrowind.bsa\n",
                "fallback-archive=Tribunal.bsa\n",
                "no-sound=1\n",
                "resources=resources\n",
            )
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_paths_does_not_include_composed_synthetic_entries() {
        let dir = unique_test_dir("user-output-only");
        let resources = dir.join("resources");
        fs::create_dir_all(resources.join("vfs")).unwrap();
        let cfg = dir.join("openmw.cfg");
        let ini = dir.join("Morrowind.ini");
        fs::write(&cfg, format!("resources={}\n", resources.display())).unwrap();
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_paths(&ini, &cfg).unwrap();
        let output = serialize_cfg(&result.cfg);

        assert!(output.contains("resources="));
        assert!(output.contains("no-sound=1"));
        assert!(!output.contains("data="));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn imports_fallback_values_with_legacy_shapes() {
        let dir = unique_test_dir("fallback-shapes");
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("openmw.cfg");
        let ini = dir.join("Morrowind.ini");
        let output = dir.join("imported.cfg");
        fs::write(&cfg, "encoding=win1252\n").unwrap();
        fs::write(
            &ini,
            concat!(
                "[Movies]\n",
                "New Game=movie,with,commas.bik\n",
                "[Weather]\n",
                "Sunrise Time=6\n",
                "Sun Glare Fader Max=0.75\n",
                "[Weather Clear]\n",
                "Sky Day Color=10,20,30\n",
            ),
        )
        .unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_paths(&ini, &cfg).unwrap();
        importer.save_config_output(&output, &result.cfg).unwrap();
        let written = fs::read_to_string(&output).unwrap();

        assert!(written.contains("fallback=Movies_New_Game,movie,with,commas.bik"));
        assert!(written.contains("fallback=Weather_Sunrise_Time,6"));
        assert!(written.contains("fallback=Weather_Sun_Glare_Fader_Max,0.75"));
        assert!(written.contains("fallback=Weather_Clear_Sky_Day_Color,10,20,30"));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_paths_missing_cfg_starts_empty() {
        let dir = unique_test_dir("import-paths-missing-cfg");
        fs::create_dir_all(&dir).unwrap();
        let cfg = dir.join("missing.cfg");
        let ini = dir.join("Morrowind.ini");
        fs::write(&ini, "[General]\nDisable Audio=1\n").unwrap();

        let importer = IniImporter::new(ImportOptions::default());
        let result = importer.import_paths(&ini, &cfg).unwrap();

        assert!(!cfg.exists());
        assert_eq!(values(&result.cfg, "no-sound"), &["1".to_owned()]);
        assert_eq!(values(&result.cfg, "encoding"), &["win1252".to_owned()]);

        fs::remove_dir_all(dir).unwrap();
    }

    fn tes3_bytes(masters: &[&str]) -> Vec<u8> {
        let masters: Vec<_> = masters.iter().map(|master| master.as_bytes()).collect();
        tes3_bytes_from_master_bytes(&masters)
    }

    fn tes3_bytes_from_master_bytes(masters: &[&[u8]]) -> Vec<u8> {
        let mut record = Vec::new();
        subrecord(&mut record, *b"HEDR", &[0; 300]);
        for master in masters {
            let mut name = (*master).to_vec();
            name.push(0);
            subrecord(&mut record, *b"MAST", &name);
            subrecord(&mut record, *b"DATA", &0u64.to_le_bytes());
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"TES3");
        bytes.extend_from_slice(&u32::try_from(record.len()).unwrap().to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&record);
        bytes
    }

    fn subrecord(output: &mut Vec<u8>, name: [u8; 4], data: &[u8]) {
        output.extend_from_slice(&name);
        output.extend_from_slice(&u32::try_from(data.len()).unwrap().to_le_bytes());
        output.extend_from_slice(data);
    }

    fn unique_test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dream-ini-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
