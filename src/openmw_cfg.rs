use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use openmw_config::{EncodingSetting, OpenMWConfiguration};

use crate::{ImportError, MultiMap};

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
    fs::write(output_path, serialize_resolved_configuration(config)).map_err(|source| {
        ImportError::Io {
            path: output_path.to_owned(),
            source,
        }
    })?;
    Ok(())
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
    configuration_from_multimap_preserving(cfg, user_config_dir)?
        .save_to_path(output_path)
        .map_err(|error| config_error(&error))
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
    remove_composed_resource_vfs_data_dir(&mut cfg);
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
    remove_composed_resource_vfs_data_dir(&mut cfg);
    Ok(cfg)
}

fn serialize_resolved_configuration(config: &OpenMWConfiguration) -> String {
    let mut cfg = crate::parse_cfg_str(&config.to_resolved_string());
    remove_composed_resource_vfs_data_dir(&mut cfg);
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

fn remove_composed_resource_vfs_data_dir(cfg: &mut MultiMap) {
    // openmw-config 1.0.5 serializes <resources>/vfs as a composed data= entry in resolved
    // output. That path is an implicit engine VFS mount derived from resources=, not a persisted
    // user data directory, so dream-ini must not write it back as data=... while the importer still
    // uses the legacy MultiMap adapter. Keep resources=... as the source of truth.
    let Some(resources) = cfg.get("resources").and_then(|values| values.last()) else {
        return;
    };
    let engine_vfs = Path::new(resources)
        .join("vfs")
        .to_string_lossy()
        .into_owned();

    if let Some(data_dirs) = cfg.get_mut("data") {
        data_dirs.retain(|data_dir| data_dir != &engine_vfs);
    }
}

fn config_error(error: &openmw_config::ConfigError) -> ImportError {
    ImportError::OpenMwConfig(error.to_string())
}
