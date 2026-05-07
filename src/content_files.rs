use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::events::ImportEvent;
use crate::plugin::{apply_morrowind_expansion_order, dependency_sort, read_plugin_header};
use crate::{Game, ImportError, MultiMap, TextEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedContentFiles {
    pub(crate) content: Vec<String>,
    pub(crate) data_dirs: Vec<PathBuf>,
    pub(crate) events: Vec<ImportEvent>,
}

pub(crate) fn import_content_files(
    ini: &MultiMap,
    cfg: &MultiMap,
    ini_path: &Path,
    game: Game,
    explicit_data_dirs: &[PathBuf],
    encoding: TextEncoding,
    verbose: bool,
) -> Result<ImportedContentFiles, ImportError> {
    let search_paths = build_search_paths(cfg, ini_path, explicit_data_dirs);
    let mut events = Vec::new();
    let mut content_files = resolve_content_files(ini, &search_paths, verbose, &mut events)?;

    let data_dirs = used_data_dirs_to_write(cfg, &content_files);
    for data_dir in &data_dirs {
        events.push(ImportEvent::DataDirAddedForContent {
            path: data_dir.clone(),
        });
    }

    content_files.sort_by(|left, right| {
        left.sort_key
            .cmp(&right.sort_key)
            .then_with(|| left.path.cmp(&right.path))
    });

    let format = game.plugin_format();
    let mut dependencies = Vec::new();
    for content_file in content_files {
        let header = read_plugin_header(&content_file.path, format, encoding)?;
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

fn build_search_paths(
    cfg: &MultiMap,
    ini_path: &Path,
    explicit_data_dirs: &[PathBuf],
) -> Vec<ContentSearchPath> {
    let mut search_paths = Vec::new();
    search_paths.extend(explicit_data_dirs.iter().map(|path| ContentSearchPath {
        path: fs::canonicalize(path).unwrap_or_else(|_| path.clone()),
        origin: SearchPathOrigin::Explicit,
    }));
    if let Some(paths) = cfg.get("data") {
        add_search_paths(&mut search_paths, paths, SearchPathOrigin::Config);
    }
    if let Some(paths) = cfg.get("data-local") {
        add_search_paths(&mut search_paths, paths, SearchPathOrigin::Config);
    }
    let default_data_path = ini_path
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join("Data Files");
    let default_data_path = fs::canonicalize(&default_data_path).unwrap_or(default_data_path);
    search_paths.push(ContentSearchPath {
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
    origin: SearchPathOrigin,
}

#[derive(Debug, Clone)]
struct ResolvedContentFile {
    sort_key: u128,
    path: PathBuf,
    data_path: PathBuf,
    search_path_origin: SearchPathOrigin,
}

fn used_data_dirs_to_write(cfg: &MultiMap, content_files: &[ResolvedContentFile]) -> Vec<PathBuf> {
    let mut used_paths: Vec<PathBuf> = Vec::new();
    for content_file in content_files {
        if content_file.search_path_origin == SearchPathOrigin::Config {
            continue;
        }
        if !has_equivalent_data_path(cfg, &content_file.data_path)
            && !used_paths
                .iter()
                .any(|path| equivalent_paths(path.as_path(), &content_file.data_path))
        {
            used_paths.push(content_file.data_path.clone());
        }
    }
    used_paths
}

fn add_search_paths(
    output: &mut Vec<ContentSearchPath>,
    input: &[String],
    origin: SearchPathOrigin,
) {
    for path in input {
        output.push(ContentSearchPath {
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
