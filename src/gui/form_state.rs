// SPDX-License-Identifier: GPL-3.0-only

#![cfg_attr(
    all(feature = "portmaster-gui", not(feature = "gui")),
    allow(dead_code)
)]

use std::path::{Path, PathBuf};

use dream_ini::{
    ImportError, ImportEvent, ImportOptions, ImportResult, ImportWarning, IniImporter,
    PreservedCfgUpdate, TextEncoding, apply_preserved_cfg_update, load_cfg_document,
    save_cfg_output_to_path, save_preserved_cfg_document_to_path,
    save_resolved_configuration_to_path, serialize_cfg_output, serialize_preserved_cfg_document,
    serialize_resolved_configuration,
};

use super::localization::UiText;
use super::path_helpers::{cfg_parent, optional_path, same_cfg_context};
use super::result_panels::ResultPanel;

#[derive(Debug, Clone)]
pub(super) struct ImportFormState {
    pub(super) morrowind_ini: String,
    pub(super) existing_cfg: String,
    pub(super) encoding: Option<TextEncoding>,
    pub(super) import_fonts: bool,
    pub(super) import_archives: bool,
    pub(super) import_content_files: bool,
    pub(super) explicit_search_path: String,
    pub(super) data_local: String,
    pub(super) resources: String,
    pub(super) user_data: String,
    pub(super) output_mode: GuiOutputMode,
    pub(super) output_path: String,
}

impl Default for ImportFormState {
    fn default() -> Self {
        Self {
            morrowind_ini: String::new(),
            existing_cfg: String::new(),
            encoding: None,
            import_fonts: false,
            import_archives: true,
            import_content_files: false,
            explicit_search_path: String::new(),
            data_local: String::new(),
            resources: String::new(),
            user_data: String::new(),
            output_mode: GuiOutputMode::PreviewOnly,
            output_path: String::new(),
        }
    }
}

impl ImportFormState {
    pub(super) fn disabled_import_reason(&self) -> Option<UiText> {
        if optional_path(&self.morrowind_ini).is_none() {
            return Some(UiText::SelectMorrowindIniBeforeImporting);
        }
        if self.output_mode == GuiOutputMode::SaveAs && optional_path(&self.output_path).is_none() {
            return Some(UiText::SelectOutputPathBeforeImporting);
        }
        if self.output_mode == GuiOutputMode::UpdateExistingCfg
            && optional_path(&self.existing_cfg).is_none()
        {
            return Some(UiText::SelectExistingCfgBeforeUpdating);
        }
        None
    }

    pub(super) fn run_import(&self) -> GuiImportResult {
        let Some(ini_path) = optional_path(&self.morrowind_ini) else {
            return GuiImportResult::Error {
                error: GuiImportError::MissingMorrowindIni,
            };
        };
        let cfg_path = optional_path(&self.existing_cfg);
        let importer = IniImporter::new(self.import_options());

        match importer.import_optional_cfg_path(&ini_path, cfg_path.as_deref()) {
            Ok(result) => {
                let cfg_text = match self.serialize_result(&result) {
                    Ok(cfg_text) => cfg_text,
                    Err(error) => {
                        return GuiImportResult::Error {
                            error: GuiImportError::Import(error),
                        };
                    }
                };
                match self.write_output(&result) {
                    Ok(output_path) => GuiImportResult::Success {
                        cfg_text,
                        warnings: result.warnings,
                        events: result.events,
                        output_path,
                    },
                    Err(error) => GuiImportResult::Error { error },
                }
            }
            Err(error) => GuiImportResult::Error {
                error: GuiImportError::Import(error),
            },
        }
    }

    pub(super) fn serialize_result(&self, result: &ImportResult) -> Result<String, ImportError> {
        if let Some(cfg_path) = optional_path(&self.existing_cfg) {
            let mut config = load_cfg_document(&cfg_path)?;
            apply_preserved_cfg_update(
                &mut config,
                &result.cfg,
                &self.preserved_update(),
                &result.changed_keys,
            )?;
            if self.relocated_existing_cfg_output() {
                Ok(serialize_resolved_configuration(&config))
            } else {
                Ok(serialize_preserved_cfg_document(
                    &config,
                    &cfg_path,
                    &self.preserved_update(),
                    &result.changed_keys,
                ))
            }
        } else {
            serialize_cfg_output(&result.cfg, &self.output_reference_dir())
        }
    }

    pub(super) fn write_output(
        &self,
        result: &ImportResult,
    ) -> Result<Option<PathBuf>, GuiImportError> {
        match self.output_mode {
            GuiOutputMode::PreviewOnly => Ok(None),
            GuiOutputMode::SaveAs => {
                let Some(output_path) = optional_path(&self.output_path) else {
                    return Err(GuiImportError::MissingOutputPath);
                };
                if let Some(cfg_path) = optional_path(&self.existing_cfg) {
                    let mut config =
                        load_cfg_document(&cfg_path).map_err(GuiImportError::Import)?;
                    apply_preserved_cfg_update(
                        &mut config,
                        &result.cfg,
                        &self.preserved_update(),
                        &result.changed_keys,
                    )
                    .map_err(GuiImportError::Import)?;
                    if same_cfg_context(&cfg_path, &output_path) {
                        save_preserved_cfg_document_to_path(
                            &config,
                            &cfg_path,
                            &output_path,
                            &self.preserved_update(),
                            &result.changed_keys,
                        )
                        .map_err(GuiImportError::Import)?;
                    } else {
                        save_resolved_configuration_to_path(&config, &output_path)
                            .map_err(GuiImportError::Import)?;
                    }
                } else {
                    save_cfg_output_to_path(&result.cfg, &output_path)
                        .map_err(GuiImportError::Import)?;
                }
                Ok(Some(output_path))
            }
            GuiOutputMode::UpdateExistingCfg => {
                let Some(cfg_path) = optional_path(&self.existing_cfg) else {
                    return Err(GuiImportError::MissingExistingCfgForUpdate);
                };
                let mut config = load_cfg_document(&cfg_path).map_err(GuiImportError::Import)?;
                apply_preserved_cfg_update(
                    &mut config,
                    &result.cfg,
                    &self.preserved_update(),
                    &result.changed_keys,
                )
                .map_err(GuiImportError::Import)?;
                save_preserved_cfg_document_to_path(
                    &config,
                    &cfg_path,
                    &cfg_path,
                    &self.preserved_update(),
                    &result.changed_keys,
                )
                .map_err(GuiImportError::Import)?;
                Ok(Some(cfg_path))
            }
        }
    }

    fn preserved_update(&self) -> PreservedCfgUpdate {
        PreservedCfgUpdate {
            import_game_files: self.import_content_files,
            import_archives: self.import_archives,
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            user_data: optional_path(&self.user_data),
        }
    }

    fn output_reference_dir(&self) -> PathBuf {
        let reference = match self.output_mode {
            GuiOutputMode::SaveAs => optional_path(&self.output_path),
            GuiOutputMode::PreviewOnly | GuiOutputMode::UpdateExistingCfg => {
                optional_path(&self.existing_cfg)
            }
        };

        reference
            .and_then(|path| path.parent().map(Path::to_owned))
            .unwrap_or_default()
    }

    pub(super) fn import_options(&self) -> ImportOptions {
        ImportOptions {
            import_game_files: self.import_content_files,
            import_fonts: self.import_fonts,
            import_archives: self.import_archives,
            data_dirs: optional_path(&self.explicit_search_path)
                .into_iter()
                .collect(),
            data_dir_base: self.output_context_dir(),
            write_resolved_data_dirs: self.relocated_existing_cfg_output(),
            data_local: optional_path(&self.data_local),
            resources: optional_path(&self.resources),
            user_data: optional_path(&self.user_data),
            encoding: self.encoding,
            verbose: true,
            ..ImportOptions::default()
        }
    }

    fn output_context_dir(&self) -> Option<PathBuf> {
        match self.output_mode {
            GuiOutputMode::SaveAs => optional_path(&self.output_path),
            GuiOutputMode::PreviewOnly | GuiOutputMode::UpdateExistingCfg => {
                optional_path(&self.existing_cfg)
            }
        }
        .map(|path| cfg_parent(&path).to_owned())
    }

    fn relocated_existing_cfg_output(&self) -> bool {
        if self.output_mode != GuiOutputMode::SaveAs {
            return false;
        }
        optional_path(&self.existing_cfg)
            .zip(optional_path(&self.output_path))
            .is_some_and(|(cfg_path, output_path)| !same_cfg_context(&cfg_path, &output_path))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum GuiOutputMode {
    #[default]
    PreviewOnly,
    SaveAs,
    UpdateExistingCfg,
}

#[derive(Debug)]
pub(super) enum GuiImportResult {
    Success {
        cfg_text: String,
        warnings: Vec<ImportWarning>,
        events: Vec<ImportEvent>,
        output_path: Option<PathBuf>,
    },
    Error {
        error: GuiImportError,
    },
}

impl GuiImportResult {
    pub(super) const fn default_panel(&self) -> ResultPanel {
        match self {
            Self::Success { .. } => ResultPanel::GeneratedCfg,
            Self::Error { .. } => ResultPanel::Errors,
        }
    }
}

#[derive(Debug)]
pub(super) enum GuiImportError {
    MissingMorrowindIni,
    MissingOutputPath,
    MissingExistingCfgForUpdate,
    Import(ImportError),
}
