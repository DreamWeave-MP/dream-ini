use std::path::{Path, PathBuf};

use openmw_config::OpenMWConfiguration;

use crate::{ImportError, MultiMap};

/// Serializes cfg entries with `OpenMW` directory semantics and resolved directory paths.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration.
pub fn serialize_resolved_cfg(
    cfg: &MultiMap,
    user_config_dir: &Path,
) -> Result<String, ImportError> {
    Ok(configuration_from_multimap(cfg, user_config_dir)?.to_resolved_string())
}

/// Writes cfg entries with `OpenMW` directory semantics and resolved directory paths.
///
/// # Errors
/// Returns [`ImportError`] if the cfg cannot be represented as an `openmw-config` configuration or
/// if writing the destination fails.
pub fn save_resolved_cfg_to_path(cfg: &MultiMap, output_path: &Path) -> Result<(), ImportError> {
    let user_config_dir = output_path.parent().unwrap_or_else(|| Path::new(""));
    configuration_from_multimap(cfg, user_config_dir)?
        .save_resolved_to_path(output_path)
        .map_err(|error| config_error(&error))
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
    Ok(crate::parse_cfg_str(&serialize_resolved_cfg(
        cfg,
        user_config_dir,
    )?))
}

fn configuration_from_multimap(
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
            "user-data" => {
                set_last_path(values, |path| config.set_user_data_path(path));
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

fn remove_composed_resource_vfs_data_dir(cfg: &mut MultiMap) {
    // openmw-config 1.0.5 serializes <resources>/vfs as a composed data= entry in resolved
    // output. That path is an implicit engine VFS mount derived from resources=, not a persisted
    // user data directory, so dream-ini must not write it back as data=... while the importer still
    // uses the legacy MultiMap adapter. Once flattened into the map we cannot distinguish that
    // synthetic entry from a deliberately authored data=<resources>/vfs entry; prefer preserving
    // OpenMW's source-of-truth spelling, resources=..., over promoting the implicit mount.
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
