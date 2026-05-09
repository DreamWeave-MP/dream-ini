// SPDX-License-Identifier: GPL-3.0-only

use dream_ini::{ImportError, ImportEvent, ImportWarning};

mod english;
mod french;
mod german;
mod russian;
mod spanish;
mod swedish;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum UiLanguage {
    #[default]
    English,
    French,
    German,
    Russian,
    Spanish,
    Swedish,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UiText {
    Language,
    EnglishLanguage,
    FrenchLanguage,
    GermanLanguage,
    RussianLanguage,
    SpanishLanguage,
    SwedishLanguage,
    SourceSection,
    Existing,
    Browse,
    ImportOptions,
    Encoding,
    EncodingAuto,
    ImportFallbacks,
    ImportArchives,
    ImportContentFiles,
    Overrides,
    ExplicitSearchPath,
    Output,
    PreviewOnly,
    SaveAs,
    OutputPath,
    UpdateExistingCfg,
    ImportPreview,
    CannotImport,
    Results,
    Errors,
    Warnings,
    Events,
    GeneratedCfg,
    Copy,
    Clear,
    EncodingTooltip,
    ImportArchivesTooltip,
    ImportContentFilesTooltip,
    ExplicitSearchPathTooltip,
    DataLocalTooltip,
    ResourcesTooltip,
    UserDataTooltip,
    NoErrors,
    NoWarnings,
    NoEvents,
    NoGeneratedCfg,
    WroteCfgTo,
    SelectMorrowindIniBeforeImporting,
    SelectOutputPathBeforeImporting,
    SelectExistingCfgBeforeUpdating,
    CancelPicker,
    ChoosePath,
    SelectPath,
    CurrentDirectory,
    ParentDirectory,
    RefreshDirectory,
    ShowHiddenDirectories,
    SelectedPath,
    OutputFileName,
    SelectMorrowindIni,
    SelectExistingOpenmwCfg,
    SelectOutputCfg,
    SelectGameDataDir,
    SelectDataLocalDir,
    SelectResourcesDir,
    SelectUserDataDir,
    ControllerHelp,
    PickerControllerHelp,
    OskTitle,
    OskControllerHelp,
    OskBackspace,
    OskClear,
    OskCancel,
    OskOk,
    OskSpace,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Localizer {
    language: UiLanguage,
}

impl Localizer {
    pub(super) const fn language(self) -> UiLanguage {
        self.language
    }

    pub(super) fn set_language(&mut self, language: UiLanguage) {
        self.language = language;
    }

    pub(super) fn text(self, key: UiText) -> &'static str {
        match self.language {
            UiLanguage::English => english::text(key),
            UiLanguage::French => french::text(key),
            UiLanguage::German => german::text(key),
            UiLanguage::Russian => russian::text(key),
            UiLanguage::Spanish => spanish::text(key),
            UiLanguage::Swedish => swedish::text(key),
        }
    }

    pub(super) fn warning_title(self, warning: &ImportWarning) -> String {
        match self.language {
            UiLanguage::English => english::warning_title(warning),
            UiLanguage::French => french::warning_title(warning),
            UiLanguage::German => german::warning_title(warning),
            UiLanguage::Russian => russian::warning_title(warning),
            UiLanguage::Spanish => spanish::warning_title(warning),
            UiLanguage::Swedish => swedish::warning_title(warning),
        }
    }

    pub(super) fn event_title(self, event: &ImportEvent) -> String {
        match self.language {
            UiLanguage::English => english::event_title(event),
            UiLanguage::French => french::event_title(event),
            UiLanguage::German => german::event_title(event),
            UiLanguage::Russian => russian::event_title(event),
            UiLanguage::Spanish => spanish::event_title(event),
            UiLanguage::Swedish => swedish::event_title(event),
        }
    }

    pub(super) fn error_title(self, error: &ImportError) -> String {
        match self.language {
            UiLanguage::English => english::error_title(error),
            UiLanguage::French => french::error_title(error),
            UiLanguage::German => german::error_title(error),
            UiLanguage::Russian => russian::error_title(error),
            UiLanguage::Spanish => spanish::error_title(error),
            UiLanguage::Swedish => swedish::error_title(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::SystemTime;

    use super::*;

    #[test]
    fn localizes_fixed_labels() {
        let localizer = Localizer::default();

        assert_eq!(localizer.text(UiText::Language), "Language");
        assert_eq!(localizer.text(UiText::PreviewOnly), "Preview only");

        let mut french = Localizer::default();
        french.set_language(UiLanguage::French);
        assert_eq!(french.text(UiText::SourceSection), "Fichiers d’entrée");

        let mut german = Localizer::default();
        german.set_language(UiLanguage::German);
        assert_eq!(german.text(UiText::SourceSection), "Eingabedateien");

        let mut russian = Localizer::default();
        russian.set_language(UiLanguage::Russian);
        assert_eq!(russian.text(UiText::SourceSection), "Исходные файлы");

        let mut swedish = Localizer::default();
        swedish.set_language(UiLanguage::Swedish);
        assert_eq!(swedish.text(UiText::SourceSection), "Källa");
    }

    #[test]
    fn localizes_structured_warnings() {
        let localizer = Localizer::default();
        let warning = ImportWarning::IgnoredEmptyValue {
            key: "Archive".to_owned(),
        };

        assert_eq!(
            localizer.warning_title(&warning),
            "Ignored empty value for key `Archive`."
        );
    }

    #[test]
    fn localizes_structured_events() {
        let localizer = Localizer::default();
        let event = ImportEvent::ContentFileResolved {
            path: PathBuf::from("Morrowind.esm"),
            modified: SystemTime::UNIX_EPOCH,
        };

        assert_eq!(
            localizer.event_title(&event),
            "Resolved content file: Morrowind.esm"
        );
    }
}
