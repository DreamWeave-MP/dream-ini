use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Language",
        UiText::EnglishLanguage => "English",
        UiText::FrenchLanguage => "Français",
        UiText::GermanLanguage => "Deutsch",
        UiText::SpanishLanguage => "Español",
        UiText::SourceSection => "Source",
        UiText::Existing => "Existing",
        UiText::Browse => "Browse…",
        UiText::ImportOptions => "Import options",
        UiText::Encoding => "Encoding",
        UiText::ImportFallbacks => "Import bitmap fonts",
        UiText::ImportArchives => "Import archives",
        UiText::ImportContentFiles => "Import content files / load order",
        UiText::Overrides => "Overrides",
        UiText::ExplicitSearchPath => "Game install path",
        UiText::Output => "Output",
        UiText::PreviewOnly => "Preview only",
        UiText::SaveAs => "Save as",
        UiText::OutputPath => "Output path",
        UiText::UpdateExistingCfg => "Update existing openmw.cfg",
        UiText::ImportPreview => "Import / Preview",
        UiText::CannotImport => "Cannot import:",
        UiText::Results => "Results",
        UiText::Errors => "Errors",
        UiText::Warnings => "Warnings",
        UiText::Events => "Events",
        UiText::GeneratedCfg => "Generated cfg",
        UiText::Copy => "Copy",
        UiText::Clear => "Clear",
        UiText::EncodingTooltip => "Character encoding used when reading Morrowind.ini text.",
        UiText::ImportArchivesTooltip => {
            "Import fallback-archive entries and resolve referenced .bsa files."
        }
        UiText::ImportContentFilesTooltip => {
            "Import GameFile entries as content load order and resolve referenced plugins."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Optional Morrowind install/Data Files search root used to resolve content and archives."
        }
        UiText::DataLocalTooltip => {
            "Override OpenMW's data-local path. This search path takes precedence over data paths."
        }
        UiText::ResourcesTooltip => {
            "Override the engine resources path. This should point at OpenMW-provided resources; choose with care."
        }
        UiText::UserdataTooltip => {
            "Override where OpenMW stores user data such as saves, screenshots, and the navmesh cache."
        }
        UiText::NoErrors => "No errors.",
        UiText::NoWarnings => "No warnings.",
        UiText::NoEvents => "No events.",
        UiText::NoGeneratedCfg => "No generated cfg.",
        UiText::WroteCfgTo => "Wrote cfg to:",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Select a Morrowind.ini file before importing."
        }
        UiText::SelectOutputPathBeforeImporting => "Select an output path before importing.",
        UiText::SelectExistingCfgBeforeUpdating => {
            "Select an existing openmw.cfg before updating in place."
        }
        UiText::CancelPicker => "Cancel",
        UiText::ChoosePath => "Choose",
        UiText::SelectPath => "Select",
        UiText::CurrentDirectory => "Current directory:",
        UiText::ParentDirectory => "Parent",
        UiText::RefreshDirectory => "Refresh",
        UiText::ShowHiddenFiles => "Show hidden files",
        UiText::SelectedPath => "Selected:",
        UiText::OutputFileName => "File name",
        UiText::SelectMorrowindIni => "Select Morrowind.ini",
        UiText::SelectExistingOpenmwCfg => "Select existing openmw.cfg",
        UiText::SelectOutputCfg => "Select output openmw.cfg",
        UiText::SelectGameDataDir => "Select game install path",
        UiText::SelectDataLocalDir => "Select data-local directory",
        UiText::SelectResourcesDir => "Select resources directory",
        UiText::SelectUserdataDir => "Select userdata directory",
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Ignored empty value for key `{key}`.")
        }
        ImportWarning::MalformedIniLine { line } => format!("Malformed INI line ignored: {line}"),
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Resolved content file: {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Resolved archive: {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!("Added data directory for content files: {}", path.display())
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Added data directory for fallback archives: {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
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
        ImportError::MissingArchives { files, .. } => {
            format!("Fallback archives not found: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Invalid content file name: {file}")
        }
        ImportError::InvalidArchiveName(file) => format!("Invalid fallback archive name: {file}"),
        _ => error.to_string(),
    }
}
