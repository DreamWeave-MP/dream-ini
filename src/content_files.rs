// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::events::ImportEvent;
use crate::{ImportError, ImportWarning, MultiMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedContentFiles {
    pub(crate) content: Vec<String>,
    pub(crate) data_dirs: Vec<DataDirToWrite>,
    pub(crate) events: Vec<ImportEvent>,
    pub(crate) warnings: Vec<ImportWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedArchives {
    pub(crate) archives: Vec<String>,
    pub(crate) data_dirs: Vec<DataDirToWrite>,
    pub(crate) events: Vec<ImportEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataDirToWrite {
    pub(crate) path: PathBuf,
    pub(crate) cfg_value: String,
}

#[derive(Clone, Copy)]
pub(crate) struct ContentFileImportRequest<'a> {
    pub(crate) ini: &'a MultiMap,
    pub(crate) cfg: &'a MultiMap,
    pub(crate) ini_path: &'a Path,
    pub(crate) cfg_dir: Option<&'a Path>,
    pub(crate) explicit_data_dirs: &'a [PathBuf],
    pub(crate) explicit_data_dir_base: Option<&'a Path>,
    pub(crate) write_resolved_data_dirs: bool,
    pub(crate) verbose: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ArchiveImportRequest<'a> {
    pub(crate) ini: &'a MultiMap,
    pub(crate) cfg: &'a MultiMap,
    pub(crate) ini_path: &'a Path,
    pub(crate) cfg_dir: Option<&'a Path>,
    pub(crate) explicit_data_dirs: &'a [PathBuf],
    pub(crate) explicit_data_dir_base: Option<&'a Path>,
    pub(crate) write_resolved_data_dirs: bool,
    pub(crate) verbose: bool,
}

pub(crate) fn import_archives(
    request: ArchiveImportRequest<'_>,
) -> Result<ImportedArchives, ImportError> {
    let search_paths = build_search_paths(
        request.cfg,
        request.ini_path,
        request.cfg_dir,
        request.explicit_data_dirs,
        request.explicit_data_dir_base,
        request.write_resolved_data_dirs,
    );
    let mut events = Vec::new();
    let archives = resolve_archives(request.ini, &search_paths, request.verbose, &mut events)?;
    let data_dirs = used_data_dirs_to_write(
        request.cfg,
        request.cfg_dir,
        &search_paths,
        archives.iter().map(|archive| archive.search_path),
    );
    for data_dir in &data_dirs {
        events.push(ImportEvent::DataDirAddedForArchive {
            path: data_dir.path.clone(),
        });
    }

    Ok(ImportedArchives {
        archives: archives.into_iter().map(|archive| archive.name).collect(),
        data_dirs,
        events,
    })
}

pub(crate) fn import_content_files(
    request: ContentFileImportRequest<'_>,
) -> Result<ImportedContentFiles, ImportError> {
    let search_paths = build_search_paths(
        request.cfg,
        request.ini_path,
        request.cfg_dir,
        request.explicit_data_dirs,
        request.explicit_data_dir_base,
        request.write_resolved_data_dirs,
    );
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut content_files = resolve_content_files(
        request.ini,
        &search_paths,
        request.verbose,
        &mut events,
        &mut warnings,
    )?;

    // Vanilla/G7 order: group, mtime, reverse case-insensitive filename, then path.
    content_files.sort_by(|left, right| {
        content_file_group(&left.name)
            .cmp(&content_file_group(&right.name))
            .then_with(|| left.sort_key.cmp(&right.sort_key))
            .then_with(|| right.lower_name.cmp(&left.lower_name))
            .then_with(|| left.path.cmp(&right.path))
    });
    let data_dirs = used_data_dirs_to_write(
        request.cfg,
        request.cfg_dir,
        &search_paths,
        content_files.iter().map(|file| file.search_path),
    );
    for data_dir in &data_dirs {
        events.push(ImportEvent::DataDirAddedForContent {
            path: data_dir.path.clone(),
        });
    }
    let content = content_files.into_iter().map(|file| file.name).collect();

    Ok(ImportedContentFiles {
        content,
        data_dirs,
        events,
        warnings,
    })
}

fn resolve_archives(
    ini: &MultiMap,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Result<Vec<ResolvedArchive>, ImportError> {
    let mut archives = Vec::new();
    let mut missing_archives = Vec::new();
    for file in archive_values(ini) {
        let file = file.trim();
        if !ends_with_ignore_ascii_case(file, ".bsa") {
            continue;
        }
        if !is_plugin_filename(file) {
            return Err(ImportError::InvalidArchiveName(file.to_owned()));
        }

        if let Some(entry) = resolve_archive(file, search_paths, verbose, events) {
            archives.push(entry);
        } else {
            missing_archives.push(file.to_owned());
        }
    }

    if missing_archives.is_empty() {
        Ok(archives)
    } else {
        Err(ImportError::MissingArchives {
            files: missing_archives,
            searched_paths: search_paths
                .iter()
                .map(|search_path| search_path.path.clone())
                .collect(),
        })
    }
}

/// `Morrowind.bsa` first, then the INI's `[Archives]` in `Archive N` order.
fn archive_values(ini: &MultiMap) -> Vec<&str> {
    let mut archives = vec!["Morrowind.bsa"];
    archives.extend(sequential_ini_values(ini, "Archives", "Archive "));
    archives
}

fn resolve_archive(
    file: &str,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Option<ResolvedArchive> {
    for (index, search_path) in search_paths.iter().enumerate() {
        if !search_path.searchable {
            continue;
        }
        let candidate = search_path.path.join(file);
        if fs::metadata(&candidate).is_ok() {
            if verbose {
                // The canonical path only feeds the event.
                let path = fs::canonicalize(&candidate).unwrap_or(candidate);
                events.push(ImportEvent::ArchiveResolved { path });
            }
            return Some(ResolvedArchive {
                name: file.to_owned(),
                search_path: index,
            });
        }
    }

    None
}

fn build_search_paths(
    cfg: &MultiMap,
    ini_path: &Path,
    cfg_dir: Option<&Path>,
    explicit_data_dirs: &[PathBuf],
    explicit_data_dir_base: Option<&Path>,
    write_resolved_data_dirs: bool,
) -> Vec<ContentSearchPath> {
    let mut search_paths = Vec::new();
    search_paths.extend(explicit_data_dirs.iter().map(|path| {
        let resolved_path = resolve_explicit_data_path(path, explicit_data_dir_base);
        let search_path = fs::canonicalize(&resolved_path).unwrap_or(resolved_path);
        let cfg_value = if write_resolved_data_dirs
            || (explicit_data_dir_base.is_none() && path.is_relative())
        {
            search_path.to_string_lossy().into_owned()
        } else {
            path.to_string_lossy().into_owned()
        };
        ContentSearchPath::new(search_path, cfg_value, SearchPathOrigin::Explicit)
    }));
    if let Some(paths) = cfg.get("data") {
        add_search_paths(&mut search_paths, paths, cfg_dir, SearchPathOrigin::Config);
    }
    let default_data_path = ini_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("Data Files");
    let default_data_path = fs::canonicalize(&default_data_path).unwrap_or(default_data_path);
    let cfg_value = default_data_path.to_string_lossy().into_owned();
    search_paths.push(ContentSearchPath::new(
        default_data_path,
        cfg_value,
        SearchPathOrigin::Default,
    ));
    search_paths
}

fn resolve_content_files(
    ini: &MultiMap,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
    warnings: &mut Vec<ImportWarning>,
) -> Result<Vec<ResolvedContentFile>, ImportError> {
    let mut content_files = Vec::new();
    for file in sequential_ini_values(ini, "Game Files", "GameFile") {
        let file = file.trim();
        if !ends_with_ignore_ascii_case(file, ".esm") && !ends_with_ignore_ascii_case(file, ".esp")
        {
            continue;
        }
        if !is_plugin_filename(file) {
            return Err(ImportError::InvalidContentFileName(file.to_owned()));
        }

        if let Some(entry) = resolve_content_file(file, search_paths, verbose, events) {
            content_files.push(entry);
        } else {
            warnings.push(ImportWarning::MissingGameFile {
                file: file.to_owned(),
            });
            break;
        }
    }

    Ok(content_files)
}

fn resolve_content_file(
    file: &str,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Option<ResolvedContentFile> {
    for (index, search_path) in search_paths.iter().enumerate() {
        if !search_path.searchable {
            continue;
        }
        let candidate = search_path.path.join(file);
        if let Ok(metadata) = fs::metadata(&candidate) {
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let path = fs::canonicalize(&candidate).unwrap_or(candidate);
            if verbose {
                events.push(ImportEvent::ContentFileResolved {
                    path: path.clone(),
                    modified,
                });
            }
            return Some(ResolvedContentFile {
                name: file.to_owned(),
                lower_name: file.to_ascii_lowercase(),
                sort_key: system_time_key(modified),
                path,
                search_path: index,
            });
        }
    }

    None
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
enum SearchPathOrigin {
    Explicit,
    Config,
    Default,
}

#[derive(Debug, Clone)]
struct ContentSearchPath {
    path: PathBuf,
    cfg_value: String,
    origin: SearchPathOrigin,
    /// Whether `path` is a directory on disk. Nothing resolves inside anything else, so probing
    /// skips the rest; error messages still list every search path.
    searchable: bool,
}

impl ContentSearchPath {
    fn new(path: PathBuf, cfg_value: String, origin: SearchPathOrigin) -> Self {
        let searchable = path.is_dir();
        ContentSearchPath {
            path,
            cfg_value,
            origin,
            searchable,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedContentFile {
    name: String,
    lower_name: String,
    sort_key: u128,
    path: PathBuf,
    /// Index of the search path the file was resolved from.
    search_path: usize,
}

#[derive(Debug, Clone)]
struct ResolvedArchive {
    name: String,
    search_path: usize,
}

/// The search paths that resolved a file and are neither cfg `data=` entries nor equivalent to
/// one (or to an earlier written path), in first-hit order. Each search path is decided once,
/// and the cfg's `data=` values are canonicalised once, on first need.
fn used_data_dirs_to_write(
    cfg: &MultiMap,
    cfg_dir: Option<&Path>,
    search_paths: &[ContentSearchPath],
    hits: impl Iterator<Item = usize>,
) -> Vec<DataDirToWrite> {
    let mut used: Vec<DataDirToWrite> = Vec::new();
    let mut used_canonical: Vec<PathBuf> = Vec::new();
    let mut decided = vec![false; search_paths.len()];
    let mut cfg_dirs: Option<Vec<PathBuf>> = None;
    for index in hits {
        let search_path = &search_paths[index];
        if search_path.origin == SearchPathOrigin::Config || decided[index] {
            continue;
        }
        decided[index] = true;
        let canonical = canonical_or_self(&search_path.path);
        let cfg_dirs = cfg_dirs.get_or_insert_with(|| {
            cfg.get("data").map_or_else(Vec::new, |values| {
                values
                    .iter()
                    .map(|value| canonical_or_self(&resolve_cfg_path(unquote_path(value), cfg_dir)))
                    .collect()
            })
        });
        if cfg_dirs.contains(&canonical) || used_canonical.contains(&canonical) {
            continue;
        }
        used.push(DataDirToWrite {
            path: search_path.path.clone(),
            cfg_value: search_path.cfg_value.clone(),
        });
        used_canonical.push(canonical);
    }
    used
}

/// The values of `{section}:{key_prefix}{N}` for `N = 0, 1, ...` up to the first index with no
/// values, keys compared ASCII case-insensitively, in one pass over the map. Several spellings
/// of one index contribute their values in map order, and only a canonical decimal (`7`, never
/// `07` or `+7`) is an index, as the C++ importer formats the keys it looks up.
fn sequential_ini_values<'a>(ini: &'a MultiMap, section: &str, key_prefix: &str) -> Vec<&'a str> {
    let prefix = format!("{section}:{key_prefix}");
    let mut indexed: BTreeMap<usize, Vec<&'a str>> = BTreeMap::new();
    for (key, values) in ini {
        let Some((head, tail)) = key.split_at_checked(prefix.len()) else {
            continue;
        };
        if values.is_empty() || !head.eq_ignore_ascii_case(&prefix) {
            continue;
        }
        if let Some(index) = canonical_index(tail) {
            indexed
                .entry(index)
                .or_default()
                .extend(values.iter().map(String::as_str));
        }
    }
    let mut sequence = Vec::new();
    for index in 0.. {
        match indexed.remove(&index) {
            Some(values) => sequence.extend(values),
            None => break,
        }
    }
    sequence
}

/// `text` as the index `usize::to_string` would print, else `None`.
fn canonical_index(text: &str) -> Option<usize> {
    let canonical = match text.as_bytes() {
        [b'0'] => true,
        [b'1'..=b'9', rest @ ..] => rest.iter().all(u8::is_ascii_digit),
        _ => false,
    };
    if canonical { text.parse().ok() } else { None }
}

fn add_search_paths(
    output: &mut Vec<ContentSearchPath>,
    input: &[String],
    cfg_dir: Option<&Path>,
    origin: SearchPathOrigin,
) {
    for path in input {
        output.push(ContentSearchPath::new(
            resolve_cfg_path(unquote_path(path), cfg_dir),
            path.clone(),
            origin,
        ));
    }
}

fn resolve_cfg_path(path: &str, cfg_dir: Option<&Path>) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_owned()
    } else if let Some(cfg_dir) = cfg_dir {
        cfg_dir.join(path)
    } else {
        path.to_owned()
    }
}

fn resolve_explicit_data_path(path: &Path, base: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else if let Some(base) = base {
        base.join(path)
    } else {
        path.to_owned()
    }
}

fn unquote_path(path: &str) -> &str {
    path.strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .unwrap_or(path)
}

fn canonical_or_self(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

fn content_file_group(file: &str) -> u8 {
    u8::from(!ends_with_ignore_ascii_case(file, ".esm"))
}

fn system_time_key(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
