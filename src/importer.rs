use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fallback_keys::MORROWIND_FALLBACK_KEYS;
use crate::parser::{
    insert_multimap, parse_cfg_str, parse_ini_bytes_with_warnings, serialize_cfg, set_single_value,
};
use crate::plugin::{apply_morrowind_expansion_order, dependency_sort, read_plugin_header};
use crate::{Game, ImportError, MultiMap, TextEncoding};

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ImportOptions {
    pub game: Game,
    pub import_game_files: bool,
    pub import_fonts: bool,
    pub import_archives: bool,
    pub data_dirs: Vec<PathBuf>,
    pub encoding: Option<TextEncoding>,
    pub verbose: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            game: Game::Morrowind,
            import_game_files: false,
            import_fonts: false,
            import_archives: true,
            data_dirs: Vec::new(),
            encoding: None,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub cfg: MultiMap,
    pub warnings: Vec<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub warnings: Vec<String>,
    pub messages: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct IniImporter {
    options: ImportOptions,
}

impl IniImporter {
    #[must_use]
    pub fn new(options: ImportOptions) -> Self {
        Self { options }
    }

    /// Imports from paths into the lightweight map model.
    ///
    /// # Errors
    /// Returns [`ImportError`] when files cannot be read, encoding is unsupported, content files
    /// cannot be resolved, or plugin headers are invalid.
    pub fn import_paths(
        &self,
        ini_path: &Path,
        cfg_path: &Path,
    ) -> Result<ImportResult, ImportError> {
        self.import_optional_cfg_path(ini_path, Some(cfg_path))
    }

    /// Imports from an INI path and an optional cfg path.
    ///
    /// # Errors
    /// Returns [`ImportError`] when files cannot be read, encoding is unsupported, content files
    /// cannot be resolved, or plugin headers are invalid.
    pub fn import_optional_cfg_path(
        &self,
        ini_path: &Path,
        cfg_path: Option<&Path>,
    ) -> Result<ImportResult, ImportError> {
        let mut cfg = match cfg_path {
            Some(path) if path.exists() => parse_cfg_str(&read_to_string(path)?),
            _ => MultiMap::new(),
        };

        let encoding = self.effective_encoding(&cfg)?;
        set_single_value(&mut cfg, "encoding", encoding.as_label().to_owned());

        let ini_bytes = read_bytes(ini_path)?;
        let parsed_ini = parse_ini_bytes_with_warnings(&ini_bytes, encoding);
        let mut report = self.import_maps(&mut cfg, &parsed_ini.entries, ini_path)?;
        report.warnings.splice(0..0, parsed_ini.warnings);
        Ok(ImportResult {
            cfg,
            warnings: report.warnings,
            messages: report.messages,
        })
    }

    /// Saves an imported configuration to an arbitrary output path.
    ///
    /// # Errors
    /// Returns [`ImportError`] when the file cannot be written.
    pub fn save_config_output(
        &self,
        output_path: &Path,
        cfg: &MultiMap,
    ) -> Result<(), ImportError> {
        fs::write(output_path, serialize_cfg(cfg)).map_err(|source| ImportError::Io {
            path: output_path.to_owned(),
            source,
        })
    }

    /// Imports already parsed maps into the lightweight map model.
    ///
    /// # Errors
    /// Returns [`ImportError`] when content files cannot be resolved or plugin headers cannot be
    /// read or decoded.
    pub fn import_maps(
        &self,
        cfg: &mut MultiMap,
        ini: &MultiMap,
        ini_path: &Path,
    ) -> Result<ImportReport, ImportError> {
        let warnings = Vec::new();
        let mut messages = Vec::new();
        let mut imported_cfg = cfg.clone();

        merge(&mut imported_cfg, ini);
        merge_fallback(&mut imported_cfg, ini, self.options.import_fonts);

        if self.options.import_game_files {
            self.import_game_files(&mut imported_cfg, ini, ini_path, &mut messages)?;
        }

        if self.options.import_archives {
            import_archives(&mut imported_cfg, ini);
        }

        *cfg = imported_cfg;
        Ok(ImportReport { warnings, messages })
    }

    fn effective_encoding(&self, cfg: &MultiMap) -> Result<TextEncoding, ImportError> {
        if let Some(encoding) = self.options.encoding {
            return Ok(encoding);
        }

        if let Some(value) = cfg.get("encoding").and_then(|values| values.last()) {
            return TextEncoding::parse(value);
        }

        Ok(TextEncoding::Win1252)
    }

    fn import_game_files(
        &self,
        cfg: &mut MultiMap,
        ini: &MultiMap,
        ini_path: &Path,
        messages: &mut Vec<String>,
    ) -> Result<(), ImportError> {
        let mut data_paths = Vec::new();
        data_paths.extend(self.options.data_dirs.iter().map(|path| DataPath {
            path: fs::canonicalize(path).unwrap_or_else(|_| path.clone()),
            origin: DataPathOrigin::Explicit,
        }));
        if let Some(paths) = cfg.get("data") {
            add_paths(&mut data_paths, paths, DataPathOrigin::Config);
        }
        if let Some(paths) = cfg.get("data-local") {
            add_paths(&mut data_paths, paths, DataPathOrigin::Config);
        }
        let default_data_path = ini_path
            .parent()
            .unwrap_or_else(|| Path::new(""))
            .join("Data Files");
        let default_data_path = fs::canonicalize(&default_data_path).unwrap_or(default_data_path);
        data_paths.push(DataPath {
            path: default_data_path.clone(),
            origin: DataPathOrigin::Default,
        });

        let mut content_files = Vec::new();
        let mut missing_content_files = Vec::new();
        for file in game_file_values(ini).into_iter().map(|file| file.trim()) {
            if !ends_with_ignore_ascii_case(file, ".esm")
                && !ends_with_ignore_ascii_case(file, ".esp")
            {
                continue;
            }
            if !is_plugin_filename(file) {
                return Err(ImportError::InvalidContentFileName(file.to_owned()));
            }

            let mut found = None;
            for data_path in &data_paths {
                let candidate = data_path.path.join(file);
                if let Ok(metadata) = fs::metadata(&candidate) {
                    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
                    let path = fs::canonicalize(&candidate).unwrap_or(candidate);
                    if self.options.verbose {
                        messages.push(format!(
                            "content file: {} timestamp = ({})",
                            path.display(),
                            system_time_seconds(modified)
                        ));
                    }
                    found = Some(ResolvedContentFile {
                        sort_key: system_time_key(modified),
                        path,
                        data_path: data_path.path.clone(),
                        data_path_origin: data_path.origin,
                    });
                    break;
                }
            }

            if let Some(entry) = found {
                content_files.push(entry);
            } else {
                missing_content_files.push(file.to_owned());
            }
        }

        if !missing_content_files.is_empty() {
            return Err(ImportError::MissingContentFiles {
                files: missing_content_files,
                searched_paths: data_paths
                    .iter()
                    .map(|data_path| data_path.path.clone())
                    .collect(),
            });
        }

        for data_path in used_non_config_data_paths(&content_files) {
            if !has_equivalent_data_path(cfg, &data_path) {
                messages.push(format!(
                    "adding data directory used to resolve content files: {}",
                    data_path.display()
                ));
                insert_multimap(
                    cfg,
                    "data".to_owned(),
                    data_path.to_string_lossy().into_owned(),
                );
            }
        }

        content_files.sort_by(|left, right| {
            left.sort_key
                .cmp(&right.sort_key)
                .then_with(|| left.path.cmp(&right.path))
        });

        let format = self.options.game.plugin_format();
        let encoding = self.effective_encoding(cfg)?;
        let mut dependencies = Vec::new();
        for content_file in content_files {
            let header = read_plugin_header(&content_file.path, format, encoding)?;
            dependencies.push((header.name, header.masters));
        }

        let mut sorted = dependency_sort(dependencies);
        apply_morrowind_expansion_order(&mut sorted);
        cfg.insert("content".to_owned(), sorted);

        Ok(())
    }
}

fn merge(cfg: &mut MultiMap, ini: &MultiMap) {
    if let Some(values) = ini.get("General:Disable Audio")
        && let Some(value) = values.last()
    {
        cfg.insert("no-sound".to_owned(), vec![value.clone()]);
    }
}

fn merge_fallback(cfg: &mut MultiMap, ini: &MultiMap, import_fonts: bool) {
    cfg.remove("fallback");
    for key in MORROWIND_FALLBACK_KEYS {
        if !import_fonts && matches!(*key, "Fonts:Font 0" | "Fonts:Font 1" | "Fonts:Font 2") {
            continue;
        }
        if let Some(values) = ini.get(*key) {
            for value in values {
                let fallback_key = key.replace([' ', ':'], "_");
                insert_multimap(
                    cfg,
                    "fallback".to_owned(),
                    format!("{fallback_key},{value}"),
                );
            }
        }
    }
}

pub(crate) fn import_archives(cfg: &mut MultiMap, ini: &MultiMap) {
    let mut archives = vec!["Morrowind.bsa".to_owned()];
    archives.extend(sequential_ini_values(ini, "Archives:Archive ").cloned());
    cfg.insert("fallback-archive".to_owned(), archives);
}

fn sequential_ini_values<'a>(ini: &'a MultiMap, prefix: &str) -> impl Iterator<Item = &'a String> {
    (0..)
        .map(move |index| format!("{prefix}{index}"))
        .map_while(move |key| ini.get(&key))
        .flat_map(|values| values.iter())
}

fn game_file_values(ini: &MultiMap) -> Vec<&String> {
    let mut values = Vec::new();
    for (key, entries) in ini {
        if let Some(index) = key
            .strip_prefix("Game Files:GameFile")
            .and_then(|suffix| suffix.parse::<usize>().ok())
        {
            for (entry_index, entry) in entries.iter().enumerate() {
                values.push((index, entry_index, entry));
            }
        }
    }
    values.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    values.into_iter().map(|(_, _, value)| value).collect()
}

fn is_plugin_filename(file: &str) -> bool {
    !file.is_empty()
        && !file.contains('/')
        && !file.contains('\\')
        && Path::new(file)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DataPathOrigin {
    Explicit,
    Config,
    Default,
}

#[derive(Debug, Clone)]
struct DataPath {
    path: PathBuf,
    origin: DataPathOrigin,
}

#[derive(Debug, Clone)]
struct ResolvedContentFile {
    sort_key: u128,
    path: PathBuf,
    data_path: PathBuf,
    data_path_origin: DataPathOrigin,
}

fn used_non_config_data_paths(content_files: &[ResolvedContentFile]) -> Vec<PathBuf> {
    let mut used_paths: Vec<PathBuf> = Vec::new();
    for content_file in content_files {
        if content_file.data_path_origin == DataPathOrigin::Config {
            continue;
        }
        if !used_paths
            .iter()
            .any(|path| equivalent_paths(path.as_path(), &content_file.data_path))
        {
            used_paths.push(content_file.data_path.clone());
        }
    }
    used_paths
}

fn add_paths(output: &mut Vec<DataPath>, input: &[String], origin: DataPathOrigin) {
    for path in input {
        output.push(DataPath {
            path: PathBuf::from(unquote_path(path)),
            origin,
        });
    }
}

fn unquote_path(path: &str) -> &str {
    path.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(path)
}

fn has_equivalent_data_path(cfg: &MultiMap, path: &Path) -> bool {
    ["data", "data-local"].iter().any(|key| {
        cfg.get(*key).is_some_and(|values| {
            values
                .iter()
                .any(|value| equivalent_paths(Path::new(unquote_path(value)), path))
        })
    })
}

fn equivalent_paths(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left == right
}

fn read_to_string(path: &Path) -> Result<String, ImportError> {
    fs::read_to_string(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    fs::read(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value.len() >= suffix.len() && value[value.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
}

fn system_time_key(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
