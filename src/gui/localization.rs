#![allow(
    dead_code,
    reason = "GUI localization keys are introduced before the first form uses all of them"
)]

use dream_ini::{ImportError, ImportEvent, ImportWarning};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum UiLanguage {
    #[default]
    English,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UiText {
    Language,
    EnglishLanguage,
    SourceSection,
    MorrowindIni,
    ExistingCfg,
    Browse,
    ImportOptions,
    ImportFallbacks,
    ImportArchives,
    ImportContentFiles,
    Overrides,
    DataDirs,
    DataLocal,
    Resources,
    Userdata,
    Output,
    PreviewOnly,
    ImportPreview,
    Results,
    Errors,
    Warnings,
    Events,
    GeneratedCfg,
    Copy,
    AddDataDir,
    RemoveDataDir,
    NoErrors,
    NoWarnings,
    NoEvents,
    NoGeneratedCfg,
    SelectMorrowindIniBeforeImporting,
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
            UiLanguage::English => english_text(key),
        }
    }

    pub(super) fn warning_title(self, warning: &ImportWarning) -> String {
        match self.language {
            UiLanguage::English => english_warning_title(warning),
        }
    }

    pub(super) fn event_title(self, event: &ImportEvent) -> String {
        match self.language {
            UiLanguage::English => english_event_title(event),
        }
    }

    pub(super) fn error_title(self, error: &ImportError) -> String {
        match self.language {
            UiLanguage::English => english_error_title(error),
        }
    }
}

const fn english_text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Language",
        UiText::EnglishLanguage => "English",
        UiText::SourceSection => "Source",
        UiText::MorrowindIni => "Morrowind.ini",
        UiText::ExistingCfg => "Existing cfg",
        UiText::Browse => "Browse…",
        UiText::ImportOptions => "Import options",
        UiText::ImportFallbacks => "Import font fallback values",
        UiText::ImportArchives => "Import archives",
        UiText::ImportContentFiles => "Import content files / load order",
        UiText::Overrides => "Overrides",
        UiText::DataDirs => "Data dirs",
        UiText::DataLocal => "data-local",
        UiText::Resources => "resources",
        UiText::Userdata => "userdata",
        UiText::Output => "Output",
        UiText::PreviewOnly => "Preview only",
        UiText::ImportPreview => "Import / Preview",
        UiText::Results => "Results",
        UiText::Errors => "Errors",
        UiText::Warnings => "Warnings",
        UiText::Events => "Events",
        UiText::GeneratedCfg => "Generated cfg",
        UiText::Copy => "Copy",
        UiText::AddDataDir => "Add data dir",
        UiText::RemoveDataDir => "Remove data dir",
        UiText::NoErrors => "No errors.",
        UiText::NoWarnings => "No warnings.",
        UiText::NoEvents => "No events.",
        UiText::NoGeneratedCfg => "No generated cfg.",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Select a Morrowind.ini file before importing."
        }
    }
}

fn english_warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Ignored empty value for key `{key}`.")
        }
        ImportWarning::MalformedIniLine { line } => format!("Malformed INI line ignored: {line}"),
    }
}

fn english_event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Resolved content file: {}", path.display())
        }
        ImportEvent::DataDirAddedForContent { path } => {
            format!("Added data directory for content files: {}", path.display())
        }
    }
}

fn english_error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!("Could not read or write {}: {source}", path.display())
        }
        ImportError::UnsupportedEncoding(value) => format!("Unsupported text encoding: {value}"),
        ImportError::InvalidPluginHeader { path, message } => {
            format!("Invalid plugin header in {}: {message}", path.display())
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Content files not found: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Invalid content file name: {file}")
        }
        _ => error.to_string(),
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
