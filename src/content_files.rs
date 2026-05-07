use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::importer::ImportOptions;
use crate::plugin::{apply_morrowind_expansion_order, dependency_sort, read_plugin_header};
use crate::{ImportError, MultiMap, TextEncoding};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportedContentFiles {
    pub(crate) content: Vec<String>,
    pub(crate) data_dirs: Vec<PathBuf>,
    pub(crate) messages: Vec<String>,
}

pub(crate) fn import_content_files(
    ini: &MultiMap,
    cfg: &MultiMap,
    ini_path: &Path,
    options: &ImportOptions,
    encoding: TextEncoding,
) -> Result<ImportedContentFiles, ImportError> {
    let data_paths = data_paths(cfg, ini_path, &options.data_dirs);
    let mut messages = Vec::new();
    let mut content_files =
        resolve_content_files(ini, &data_paths, options.verbose, &mut messages)?;

    let data_dirs = data_dirs_to_add(cfg, &content_files);
    for data_dir in &data_dirs {
        messages.push(format!(
            "adding data directory used to resolve content files: {}",
            data_dir.display()
        ));
    }

    content_files.sort_by(|left, right| {
        left.sort_key
            .cmp(&right.sort_key)
            .then_with(|| left.path.cmp(&right.path))
    });

    let format = options.game.plugin_format();
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
        messages,
    })
}

fn data_paths(cfg: &MultiMap, ini_path: &Path, explicit_data_dirs: &[PathBuf]) -> Vec<DataPath> {
    let mut data_paths = Vec::new();
    data_paths.extend(explicit_data_dirs.iter().map(|path| DataPath {
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
        path: default_data_path,
        origin: DataPathOrigin::Default,
    });
    data_paths
}

fn resolve_content_files(
    ini: &MultiMap,
    data_paths: &[DataPath],
    verbose: bool,
    messages: &mut Vec<String>,
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

        if let Some(entry) = resolve_content_file(file, data_paths, verbose, messages) {
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
            searched_paths: data_paths
                .iter()
                .map(|data_path| data_path.path.clone())
                .collect(),
        })
    }
}

fn resolve_content_file(
    file: &str,
    data_paths: &[DataPath],
    verbose: bool,
    messages: &mut Vec<String>,
) -> Option<ResolvedContentFile> {
    for data_path in data_paths {
        let candidate = data_path.path.join(file);
        if let Ok(metadata) = fs::metadata(&candidate) {
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            let path = fs::canonicalize(&candidate).unwrap_or(candidate);
            if verbose {
                messages.push(format!(
                    "content file: {} timestamp = ({})",
                    path.display(),
                    system_time_seconds(modified)
                ));
            }
            return Some(ResolvedContentFile {
                sort_key: system_time_key(modified),
                path,
                data_path: data_path.path.clone(),
                data_path_origin: data_path.origin,
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

fn data_dirs_to_add(cfg: &MultiMap, content_files: &[ResolvedContentFile]) -> Vec<PathBuf> {
    let mut used_paths: Vec<PathBuf> = Vec::new();
    for content_file in content_files {
        if content_file.data_path_origin == DataPathOrigin::Config {
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

fn system_time_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
