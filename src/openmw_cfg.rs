use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use openmw_config::{EncodingSetting, OpenMWConfiguration};

use crate::{ImportError, MultiMap};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Serializes cfg entries with `OpenMW` directory semantics and resolved directory paths.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration.
pub fn serialize_resolved_cfg(
    cfg: &MultiMap,
    user_config_dir: &Path,
) -> Result<String, ImportError> {
    Ok(serialize_resolved_configuration(
        &configuration_from_multimap_resolved(cfg, user_config_dir)?,
    ))
}

/// Writes cfg entries with `OpenMW` directory semantics and resolved directory paths.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration or
/// if writing the destination fails.
pub fn save_resolved_cfg_to_path(cfg: &MultiMap, output_path: &Path) -> Result<(), ImportError> {
    let user_config_dir = output_path.parent().unwrap_or_else(|| Path::new(""));
    save_resolved_configuration_to_path(
        &configuration_from_multimap_resolved(cfg, user_config_dir)?,
        output_path,
    )
}

/// Writes an `openmw-config` document with resolved paths without persisting composed engine VFS
/// data directories.
///
/// # Errors
/// Returns [`ImportError`] if serializing or writing the destination fails.
pub fn save_resolved_configuration_to_path(
    config: &OpenMWConfiguration,
    output_path: &Path,
) -> Result<(), ImportError> {
    write_atomic(
        output_path,
        serialize_resolved_configuration(config).as_bytes(),
    )?;
    Ok(())
}

/// Writes the cfg layer that was loaded from `source_path` without flattening inherited configs.
///
/// # Errors
/// Returns [`ImportError`] if writing the destination fails.
pub fn save_preserved_cfg_document_to_path(
    config: &OpenMWConfiguration,
    source_path: &Path,
    output_path: &Path,
    update: &PreservedCfgUpdate,
    changed_keys: &BTreeSet<String>,
) -> Result<(), ImportError> {
    write_atomic(
        output_path,
        serialize_preserved_cfg_document(config, source_path, update, changed_keys).as_bytes(),
    )?;
    Ok(())
}

#[must_use]
pub fn serialize_preserved_cfg_document(
    config: &OpenMWConfiguration,
    source_path: &Path,
    update: &PreservedCfgUpdate,
    changed_keys: &BTreeSet<String>,
) -> String {
    let source_path = source_identity_path(source_path);
    let mut write_keys = changed_keys.clone();
    if update.data_local.is_some() {
        write_keys.insert("data-local".to_owned());
    }
    if update.resources.is_some() {
        write_keys.insert("resources".to_owned());
    }
    if update.user_data.is_some() {
        write_keys.insert("user-data".to_owned());
    }
    let user_config_path = config.user_config_path().join("openmw.cfg");
    let mut document = String::new();
    for setting in config.settings_matching(|setting| {
        let source = setting.meta().source_config();
        source == source_path.as_path()
            || (source == user_config_path
                && setting_key(setting).is_some_and(|key| write_keys.contains(&key)))
    }) {
        document.push_str(&setting.to_string());
    }
    document
}

fn source_identity_path(source_path: &Path) -> PathBuf {
    fs::canonicalize(source_path).unwrap_or_else(|_| source_path.to_path_buf())
}

fn setting_key(setting: &impl ToString) -> Option<String> {
    let text = setting.to_string();
    text.lines()
        .last()?
        .split_once('=')
        .map(|(key, _)| key.to_owned())
}

/// Import changes that should be applied to a preserving `openmw.cfg` document.
#[derive(Debug, Clone)]
pub struct PreservedCfgUpdate {
    pub import_game_files: bool,
    pub import_archives: bool,
    pub data_local: Option<PathBuf>,
    pub resources: Option<PathBuf>,
    pub user_data: Option<PathBuf>,
}

/// Loads an `OpenMW` cfg document without flattening it through resolved serialization.
///
/// # Errors
/// Returns [`ImportError`] if `openmw-config` cannot load the requested cfg chain.
pub fn load_cfg_document(path: &Path) -> Result<OpenMWConfiguration, ImportError> {
    OpenMWConfiguration::load_optional(path).map_err(|error| config_error(&error))
}

/// Serializes cfg entries with `OpenMW` directory semantics while preserving authored path spelling.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration.
pub fn serialize_cfg_output(cfg: &MultiMap, user_config_dir: &Path) -> Result<String, ImportError> {
    Ok(configuration_from_multimap_preserving(cfg, user_config_dir)?.to_string())
}

/// Writes cfg entries with `OpenMW` directory semantics while preserving authored path spelling.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration or
/// if writing the destination fails.
pub fn save_cfg_output_to_path(cfg: &MultiMap, output_path: &Path) -> Result<(), ImportError> {
    let user_config_dir = output_path.parent().unwrap_or_else(|| Path::new(""));
    write_atomic(
        output_path,
        configuration_from_multimap_preserving(cfg, user_config_dir)?
            .to_string()
            .as_bytes(),
    )?;
    Ok(())
}

/// Applies imported cfg values to an existing preserving `openmw-config` document.
///
/// # Errors
/// Returns [`ImportError`] if fallback or encoding values cannot be represented by
/// `openmw-config`.
pub fn apply_preserved_cfg_update(
    config: &mut OpenMWConfiguration,
    imported_cfg: &MultiMap,
    update: &PreservedCfgUpdate,
    changed_keys: &BTreeSet<String>,
) -> Result<(), ImportError> {
    if changed_keys.contains("encoding")
        && let Some(encoding) = imported_cfg
            .get("encoding")
            .and_then(|values| values.last())
    {
        set_encoding(config, encoding)?;
    }
    if changed_keys.contains("no-sound") {
        config.set_generic_settings("no-sound", imported_cfg.get("no-sound").cloned());
    }
    if changed_keys.contains("fallback") {
        config
            .set_game_settings(imported_cfg.get("fallback").cloned())
            .map_err(|error| config_error(&error))?;
    }

    if changed_keys.contains("data") {
        for data_dir in imported_cfg.get("data").into_iter().flatten() {
            if !config.has_data_dir(data_dir) {
                config.add_data_directory(Path::new(data_dir));
            }
        }
    }

    if update.import_game_files && changed_keys.contains("content") {
        config.set_content_files(imported_cfg.get("content").cloned());
    }
    if update.import_archives && changed_keys.contains("fallback-archive") {
        config.set_fallback_archives(imported_cfg.get("fallback-archive").cloned());
    }
    if let Some(path) = &update.data_local {
        clear_preserved_key(config, "data-local");
        config.set_data_local_path(path);
    }
    if let Some(path) = &update.resources {
        clear_preserved_key(config, "resources");
        config.set_resources_path(path);
    }
    if let Some(path) = &update.user_data {
        clear_preserved_key(config, "user-data");
        config.set_user_data_path(path);
    }

    Ok(())
}

pub(crate) fn load_resolved_cfg(path: &Path) -> Result<MultiMap, ImportError> {
    let config = OpenMWConfiguration::load_optional(path).map_err(|error| config_error(&error))?;
    let mut cfg = crate::parse_cfg_str(&config.to_resolved_string());
    remove_composed_non_import_data_dirs(&mut cfg);
    Ok(cfg)
}

pub(crate) fn normalize_cfg(
    cfg: &MultiMap,
    user_config_dir: Option<&Path>,
) -> Result<MultiMap, ImportError> {
    let Some(user_config_dir) = user_config_dir else {
        return Ok(cfg.clone());
    };
    let mut cfg = crate::parse_cfg_str(
        &configuration_from_multimap_resolved(cfg, user_config_dir)?.to_resolved_string(),
    );
    remove_composed_non_import_data_dirs(&mut cfg);
    Ok(cfg)
}

#[must_use]
pub fn serialize_resolved_configuration(config: &OpenMWConfiguration) -> String {
    let mut cfg = crate::parse_cfg_str(&config.to_resolved_string());
    remove_composed_non_import_data_dirs(&mut cfg);
    crate::serialize_cfg(&cfg)
}

fn configuration_from_multimap_preserving(
    cfg: &MultiMap,
    user_config_dir: &Path,
) -> Result<OpenMWConfiguration, ImportError> {
    let user_config_dir = effective_user_config_dir(user_config_dir);
    let mut config =
        OpenMWConfiguration::new_empty(&user_config_dir).map_err(|error| config_error(&error))?;

    for (key, values) in cfg {
        match key.as_str() {
            "data" => config.set_data_directories(Some(paths(values))),
            "data-local" | "resources" | "user-data" => {
                config.set_generic_settings(key, Some(values.clone()));
            }
            "content" => config.set_content_files(Some(values.clone())),
            "fallback-archive" => config.set_fallback_archives(Some(values.clone())),
            "fallback" => config
                .set_game_settings(Some(values.clone()))
                .map_err(|error| config_error(&error))?,
            other => config.set_generic_settings(other, Some(values.clone())),
        }
    }

    Ok(config)
}

fn configuration_from_multimap_resolved(
    cfg: &MultiMap,
    user_config_dir: &Path,
) -> Result<OpenMWConfiguration, ImportError> {
    let user_config_dir = effective_user_config_dir(user_config_dir);
    let mut config =
        OpenMWConfiguration::new_empty(&user_config_dir).map_err(|error| config_error(&error))?;

    for (key, values) in cfg {
        match key.as_str() {
            "data" => config.set_data_directories(Some(paths(values))),
            "data-local" => set_last_path(values, |path| config.set_data_local_path(path)),
            "resources" => set_last_path(values, |path| config.set_resources_path(path)),
            "user-data" => set_last_path(values, |path| config.set_user_data_path(path)),
            "content" => config.set_content_files(Some(values.clone())),
            "fallback-archive" => config.set_fallback_archives(Some(values.clone())),
            "fallback" => config
                .set_game_settings(Some(values.clone()))
                .map_err(|error| config_error(&error))?,
            other => config.set_generic_settings(other, Some(values.clone())),
        }
    }

    Ok(config)
}

fn effective_user_config_dir(path: &Path) -> PathBuf {
    if path.as_os_str().is_empty() {
        return std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    }

    path.to_owned()
}

fn paths(values: &[String]) -> Vec<PathBuf> {
    values.iter().map(PathBuf::from).collect()
}

fn set_last_path<F>(values: &[String], mut set: F)
where
    F: FnMut(&Path),
{
    if let Some(value) = values.last() {
        set(Path::new(value));
    }
}

fn set_encoding(config: &mut OpenMWConfiguration, encoding: &str) -> Result<(), ImportError> {
    clear_preserved_key(config, "encoding");
    let cfg_path = config.user_config_path().join("openmw.cfg");
    let mut comment = String::new();
    let setting = EncodingSetting::try_from((encoding.to_owned(), cfg_path, &mut comment))
        .map_err(|error| config_error(&error))?;
    config.set_encoding(Some(setting));
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), ImportError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    for _ in 0..16 {
        let temp_path = temporary_path_for(path);
        let file = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(ImportError::Io {
                    path: path.to_owned(),
                    source,
                });
            }
        };

        if let Err(source) = finish_atomic_write(path, parent, &temp_path, file, bytes) {
            let _ = fs::remove_file(&temp_path);
            return Err(ImportError::Io {
                path: path.to_owned(),
                source,
            });
        }

        return Ok(());
    }

    Err(ImportError::Io {
        path: path.to_owned(),
        source: io::Error::new(
            ErrorKind::AlreadyExists,
            "could not create a unique temporary cfg file",
        ),
    })
}

fn finish_atomic_write(
    path: &Path,
    parent: &Path,
    temp_path: &Path,
    mut file: fs::File,
    bytes: &[u8],
) -> io::Result<()> {
    if let Ok(metadata) = fs::metadata(path) {
        // Preserve the portable permission bits we can apply before replacement. This is not a
        // promise to preserve ownership, ACLs, xattrs, or timestamps; atomic replacement creates a
        // new file object, because of course it does.
        file.set_permissions(metadata.permissions())?;
    }
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    replace_file(temp_path, path)?;
    sync_parent_dir(parent)
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    unsafe extern "system" {
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }

    let source = wide_null(source);
    let destination = wide_null(destination);
    let result = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn wide_null(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain([0]).collect()
}

fn temporary_path_for(path: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("openmw.cfg");
    let temp_name = format!(
        ".{file_name}.dream-ini-{}-{counter}.tmp",
        std::process::id()
    );
    path.with_file_name(temp_name)
}

#[cfg(unix)]
fn sync_parent_dir(parent: &Path) -> io::Result<()> {
    fs::File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_dir(_parent: &Path) -> io::Result<()> {
    Ok(())
}

fn clear_preserved_key(config: &mut OpenMWConfiguration, key: &str) {
    let prefix = format!("{key}=");
    config.clear_matching(|setting| {
        setting
            .to_string()
            .lines()
            .last()
            .is_some_and(|line| line.starts_with(&prefix))
    });
}

fn remove_composed_non_import_data_dirs(cfg: &mut MultiMap) {
    // openmw-config 1.0.5 serializes some singleton directory settings as composed data= entries
    // in resolved output. They are not Morrowind.exe content inputs for dream-ini and must not be
    // persisted as authored data= entries. Keep their singleton keys as the source of truth.
    remove_composed_data_dir(cfg, "data-local", Path::to_owned);
    remove_composed_data_dir(cfg, "resources", |path| path.join("vfs"));
}

fn remove_composed_data_dir<F>(cfg: &mut MultiMap, key: &str, mut composed_path: F)
where
    F: FnMut(&Path) -> PathBuf,
{
    let Some(value) = cfg.get(key).and_then(|values| values.last()) else {
        return;
    };
    let composed = composed_path(Path::new(value))
        .to_string_lossy()
        .into_owned();

    if let Some(data_dirs) = cfg.get_mut("data") {
        data_dirs.retain(|data_dir| data_dir != &composed);
    }
}

fn config_error(error: &openmw_config::ConfigError) -> ImportError {
    ImportError::OpenMwConfig(error.to_string())
}
