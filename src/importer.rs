use std::fs;
use std::path::{Path, PathBuf};

use crate::content_files::{
    ArchiveImportRequest, ContentFileImportRequest, import_archives, import_content_files,
};
use crate::events::ImportEvent;
use crate::fallback_keys::MORROWIND_FALLBACK_KEYS;
use crate::openmw_cfg::{load_resolved_cfg, normalize_cfg};
use crate::parser::{insert_multimap, parse_ini_bytes_with_warnings, set_single_value};
use crate::{Game, ImportError, ImportWarning, MultiMap, TextEncoding};

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ImportOptions {
    pub game: Game,
    pub import_game_files: bool,
    pub import_fonts: bool,
    pub import_archives: bool,
    pub data_dirs: Vec<PathBuf>,
    pub data_local: Option<PathBuf>,
    pub resources: Option<PathBuf>,
    pub user_data: Option<PathBuf>,
    pub cfg_dir: Option<PathBuf>,
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
            data_local: None,
            resources: None,
            user_data: None,
            cfg_dir: None,
            encoding: None,
            verbose: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportResult {
    pub cfg: MultiMap,
    pub warnings: Vec<ImportWarning>,
    pub events: Vec<ImportEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub warnings: Vec<ImportWarning>,
    pub events: Vec<ImportEvent>,
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
            Some(path) => load_resolved_cfg(path)?,
            _ => MultiMap::new(),
        };
        let cfg_dir = cfg_path.and_then(cfg_parent_dir);

        let encoding = self.effective_encoding(&cfg)?;
        set_single_value(&mut cfg, "encoding", encoding.as_label().to_owned());

        let ini_bytes = read_bytes(ini_path)?;
        let parsed_ini = parse_ini_bytes_with_warnings(&ini_bytes, encoding);
        let mut report = self.import_maps_with_cfg_dir(
            &mut cfg,
            &parsed_ini.entries,
            ini_path,
            cfg_dir.as_deref(),
        )?;
        report.warnings.splice(0..0, parsed_ini.warnings);
        Ok(ImportResult {
            cfg,
            warnings: report.warnings,
            events: report.events,
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
        self.import_maps_with_cfg_dir(cfg, ini, ini_path, self.options.cfg_dir.as_deref())
    }

    fn import_maps_with_cfg_dir(
        &self,
        cfg: &mut MultiMap,
        ini: &MultiMap,
        ini_path: &Path,
        cfg_dir: Option<&Path>,
    ) -> Result<ImportReport, ImportError> {
        let warnings = Vec::new();
        let mut events = Vec::new();
        let mut imported_cfg = normalize_cfg(cfg, cfg_dir)?;

        merge(&mut imported_cfg, ini);
        merge_fallback(&mut imported_cfg, ini, self.options.import_fonts);

        if self.options.import_game_files {
            let encoding = self.effective_encoding(&imported_cfg)?;
            let imported_content = import_content_files(ContentFileImportRequest {
                ini,
                cfg: &imported_cfg,
                ini_path,
                cfg_dir,
                game: self.options.game,
                explicit_data_dirs: &self.options.data_dirs,
                encoding,
                verbose: self.options.verbose,
            })?;
            for data_dir in imported_content.data_dirs {
                insert_multimap(
                    &mut imported_cfg,
                    "data".to_owned(),
                    data_dir.to_string_lossy().into_owned(),
                );
            }
            imported_cfg.insert("content".to_owned(), imported_content.content);
            events.extend(imported_content.events);
        }

        if self.options.import_archives {
            let imported_archives = import_archives(ArchiveImportRequest {
                ini,
                cfg: &imported_cfg,
                ini_path,
                cfg_dir,
                explicit_data_dirs: &self.options.data_dirs,
                verbose: self.options.verbose,
            })?;
            for data_dir in imported_archives.data_dirs {
                insert_multimap(
                    &mut imported_cfg,
                    "data".to_owned(),
                    data_dir.to_string_lossy().into_owned(),
                );
            }
            imported_cfg.insert("fallback-archive".to_owned(), imported_archives.archives);
            events.extend(imported_archives.events);
        }

        self.apply_singleton_path_overrides(&mut imported_cfg);

        *cfg = imported_cfg;
        Ok(ImportReport { warnings, events })
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

    fn apply_singleton_path_overrides(&self, cfg: &mut MultiMap) {
        set_path_override(cfg, "data-local", self.options.data_local.as_deref());
        set_path_override(cfg, "resources", self.options.resources.as_deref());
        set_path_override(cfg, "user-data", self.options.user_data.as_deref());
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

fn set_path_override(cfg: &mut MultiMap, key: &str, path: Option<&Path>) {
    if let Some(path) = path {
        set_single_value(cfg, key, path.to_string_lossy().into_owned());
    }
}

fn cfg_parent_dir(path: &Path) -> Option<PathBuf> {
    path.parent().map(Path::to_owned)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, ImportError> {
    fs::read(path).map_err(|source| ImportError::Io {
        path: path.to_owned(),
        source,
    })
}
