// SPDX-License-Identifier: GPL-3.0-only

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::events::ImportEvent;
use crate::plugin::{apply_morrowind_expansion_order, dependency_sort, read_plugin_header};
use crate::{Game, ImportError, MultiMap, TextEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedContentFiles {
    pub(crate) content: Vec<String>,
    pub(crate) data_dirs: Vec<DataDirToWrite>,
    pub(crate) events: Vec<ImportEvent>,
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
    pub(crate) game: Game,
    pub(crate) explicit_data_dirs: &'a [PathBuf],
    pub(crate) explicit_data_dir_base: Option<&'a Path>,
    pub(crate) write_resolved_data_dirs: bool,
    pub(crate) encoding: TextEncoding,
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
    let data_dirs = used_archive_data_dirs_to_write(request.cfg, request.cfg_dir, &archives);
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
    let mut content_files =
        resolve_content_files(request.ini, &search_paths, request.verbose, &mut events)?;

    let data_dirs = used_data_dirs_to_write(request.cfg, request.cfg_dir, &content_files);
    for data_dir in &data_dirs {
        events.push(ImportEvent::DataDirAddedForContent {
            path: data_dir.path.clone(),
        });
    }

    content_files.sort_by(|left, right| {
        left.sort_key
            .cmp(&right.sort_key)
            .then_with(|| left.path.cmp(&right.path))
    });

    let format = request.game.plugin_format();
    let mut dependencies = Vec::new();
    for content_file in content_files {
        let header = read_plugin_header(&content_file.path, format, request.encoding)?;
        dependencies.push((header.name, header.masters));
    }

    let mut content = dependency_sort(dependencies);
    apply_morrowind_expansion_order(&mut content);

    Ok(ImportedContentFiles {
        content,
        data_dirs,
        events,
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

fn archive_values(ini: &MultiMap) -> Vec<String> {
    let mut archives = vec!["Morrowind.bsa".to_owned()];
    archives.extend(sequential_ini_values(ini, "Archives:Archive ").cloned());
    archives
}

fn resolve_archive(
    file: &str,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Option<ResolvedArchive> {
    for search_path in search_paths {
        let candidate = search_path.path.join(file);
        if fs::metadata(&candidate).is_ok() {
            let path = fs::canonicalize(&candidate).unwrap_or(candidate);
            if verbose {
                events.push(ImportEvent::ArchiveResolved { path });
            }
            return Some(ResolvedArchive {
                name: file.to_owned(),
                data_path: search_path.path.clone(),
                data_cfg_value: search_path.cfg_value.clone(),
                search_path_origin: search_path.origin,
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
        ContentSearchPath {
            path: search_path,
            cfg_value,
            origin: SearchPathOrigin::Explicit,
        }
    }));
    if let Some(paths) = cfg.get("data") {
        add_search_paths(&mut search_paths, paths, cfg_dir, SearchPathOrigin::Config);
    }
    let default_data_path = ini_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("Data Files");
    let default_data_path = fs::canonicalize(&default_data_path).unwrap_or(default_data_path);
    search_paths.push(ContentSearchPath {
        cfg_value: default_data_path.to_string_lossy().into_owned(),
        path: default_data_path,
        origin: SearchPathOrigin::Default,
    });
    search_paths
}

fn resolve_content_files(
    ini: &MultiMap,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Result<Vec<ResolvedContentFile>, ImportError> {
    let mut content_files = Vec::new();
    let mut missing_content_files = Vec::new();
    for file in game_file_values(ini).into_iter().map(|file| file.trim()) {
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
            missing_content_files.push(file.to_owned());
        }
    }

    if missing_content_files.is_empty() {
        Ok(content_files)
    } else {
        Err(ImportError::MissingContentFiles {
            files: missing_content_files,
            searched_paths: search_paths
                .iter()
                .map(|search_path| search_path.path.clone())
                .collect(),
        })
    }
}

fn resolve_content_file(
    file: &str,
    search_paths: &[ContentSearchPath],
    verbose: bool,
    events: &mut Vec<ImportEvent>,
) -> Option<ResolvedContentFile> {
    for search_path in search_paths {
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
                sort_key: system_time_key(modified),
                path,
                data_path: search_path.path.clone(),
                data_cfg_value: search_path.cfg_value.clone(),
                search_path_origin: search_path.origin,
            });
        }
    }

    None
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
}

#[derive(Debug, Clone)]
struct ResolvedContentFile {
    sort_key: u128,
    path: PathBuf,
    data_path: PathBuf,
    data_cfg_value: String,
    search_path_origin: SearchPathOrigin,
}

#[derive(Debug, Clone)]
struct ResolvedArchive {
    name: String,
    data_path: PathBuf,
    data_cfg_value: String,
    search_path_origin: SearchPathOrigin,
}

fn used_data_dirs_to_write(
    cfg: &MultiMap,
    cfg_dir: Option<&Path>,
    content_files: &[ResolvedContentFile],
) -> Vec<DataDirToWrite> {
    let mut used_paths: Vec<DataDirToWrite> = Vec::new();
    for content_file in content_files {
        if content_file.search_path_origin == SearchPathOrigin::Config {
            continue;
        }
        if !has_equivalent_data_path(cfg, cfg_dir, &content_file.data_path)
            && !used_paths
                .iter()
                .any(|path| equivalent_paths(path.path.as_path(), &content_file.data_path))
        {
            used_paths.push(DataDirToWrite {
                path: content_file.data_path.clone(),
                cfg_value: content_file.data_cfg_value.clone(),
            });
        }
    }
    used_paths
}

fn used_archive_data_dirs_to_write(
    cfg: &MultiMap,
    cfg_dir: Option<&Path>,
    archives: &[ResolvedArchive],
) -> Vec<DataDirToWrite> {
    let mut used_paths: Vec<DataDirToWrite> = Vec::new();
    for archive in archives {
        if archive.search_path_origin == SearchPathOrigin::Config {
            continue;
        }
        if !has_equivalent_data_path(cfg, cfg_dir, &archive.data_path)
            && !used_paths
                .iter()
                .any(|path| equivalent_paths(path.path.as_path(), &archive.data_path))
        {
            used_paths.push(DataDirToWrite {
                path: archive.data_path.clone(),
                cfg_value: archive.data_cfg_value.clone(),
            });
        }
    }
    used_paths
}

fn sequential_ini_values<'a>(ini: &'a MultiMap, prefix: &str) -> impl Iterator<Item = &'a String> {
    (0..)
        .map(move |index| format!("{prefix}{index}"))
        .map_while(move |key| ini.get(&key))
        .flat_map(|values| values.iter())
}

fn add_search_paths(
    output: &mut Vec<ContentSearchPath>,
    input: &[String],
    cfg_dir: Option<&Path>,
    origin: SearchPathOrigin,
) {
    for path in input {
        output.push(ContentSearchPath {
            path: resolve_cfg_path(unquote_path(path), cfg_dir),
            cfg_value: path.clone(),
            origin,
        });
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

fn has_equivalent_data_path(cfg: &MultiMap, cfg_dir: Option<&Path>, path: &Path) -> bool {
    cfg.get("data").is_some_and(|values| {
        values
            .iter()
            .any(|value| equivalent_paths(&resolve_cfg_path(unquote_path(value), cfg_dir), path))
    })
}

fn equivalent_paths(left: &Path, right: &Path) -> bool {
    let left = fs::canonicalize(left).unwrap_or_else(|_| left.to_owned());
    let right = fs::canonicalize(right).unwrap_or_else(|_| right.to_owned());
    left == right
}

fn ends_with_ignore_ascii_case(value: &str, suffix: &str) -> bool {
    value
        .get(value.len().saturating_sub(suffix.len())..)
        .is_some_and(|tail| tail.eq_ignore_ascii_case(suffix))
}

fn system_time_key(time: SystemTime) -> u128 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
