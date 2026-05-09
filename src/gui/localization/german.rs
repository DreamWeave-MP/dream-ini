// SPDX-License-Identifier: GPL-3.0-only

use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Sprache",
        UiText::EnglishLanguage => "Englisch",
        UiText::FrenchLanguage => "Französisch",
        UiText::GermanLanguage => "Deutsch",
        UiText::RussianLanguage => "Russisch",
        UiText::SpanishLanguage => "Spanisch",
        UiText::SourceSection => "Eingabedateien",
        UiText::Existing => "Vorhanden",
        UiText::Browse => "Durchsuchen…",
        UiText::ImportOptions => "Importoptionen",
        UiText::Encoding => "Kodierung",
        UiText::EncodingAuto => "Auto",
        UiText::ImportFallbacks => "Bitmap-Schriftarten importieren",
        UiText::ImportArchives => "Archive importieren",
        UiText::ImportContentFiles => "Inhaltsdateien / Ladereihenfolge importieren",
        UiText::Overrides => "Überschreibungen",
        UiText::ExplicitSearchPath => "Data-Files-Verzeichnis",
        UiText::Output => "Ausgabe",
        UiText::PreviewOnly => "Nur Vorschau",
        UiText::SaveAs => "Speichern unter",
        UiText::OutputPath => "Ausgabepfad",
        UiText::UpdateExistingCfg => "Vorhandene openmw.cfg aktualisieren",
        UiText::ImportPreview => "Importieren / Vorschau",
        UiText::CannotImport => "Import nicht möglich:",
        UiText::Results => "Ergebnisse",
        UiText::Errors => "Fehler",
        UiText::Warnings => "Warnungen",
        UiText::Events => "Ereignisse",
        UiText::GeneratedCfg => "Generierte cfg",
        UiText::Copy => "Kopieren",
        UiText::Clear => "Leeren",
        UiText::EncodingTooltip => {
            "Zeichenkodierung zum Lesen von Inhalts- und Plugin-Text. Auto verwendet die Kodierung der vorhandenen cfg oder win1252, wenn keine gesetzt ist."
        }
        UiText::ImportArchivesTooltip => {
            "Importiert fallback-archive-Einträge und löst referenzierte .bsa-Dateien auf."
        }
        UiText::ImportContentFilesTooltip => {
            "Importiert GameFile-Einträge als Ladereihenfolge und löst referenzierte Plugins auf."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Optionales Morrowind-Data-Files-Verzeichnis zum Auflösen importierter Inhalte und Archive."
        }
        UiText::DataLocalTooltip => {
            "Schreibt OpenMWs runtime data-local-Einstellung. dream-ini durchsucht diesen Pfad beim Import nicht; dafür das Data-Files-Verzeichnis verwenden."
        }
        UiText::ResourcesTooltip => {
            "Überschreibt den Pfad für Engine-Ressourcen. Er sollte auf von OpenMW bereitgestellte Ressourcen zeigen; sorgfältig auswählen."
        }
        UiText::UserDataTooltip => {
            "Überschreibt den Speicherort für OpenMW-Benutzerdaten wie Spielstände, Screenshots und Navmesh-Cache."
        }
        UiText::NoErrors => "Keine Fehler.",
        UiText::NoWarnings => "Keine Warnungen.",
        UiText::NoEvents => "Keine Ereignisse.",
        UiText::NoGeneratedCfg => "Keine cfg generiert.",
        UiText::WroteCfgTo => "Cfg geschrieben nach:",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Wählen Sie vor dem Importieren eine Morrowind.ini-Datei aus."
        }
        UiText::SelectOutputPathBeforeImporting => {
            "Wählen Sie vor dem Importieren einen Ausgabepfad aus."
        }
        UiText::SelectExistingCfgBeforeUpdating => {
            "Wählen Sie eine vorhandene openmw.cfg aus, bevor Sie sie direkt aktualisieren."
        }
        UiText::CancelPicker => "Abbrechen",
        UiText::ChoosePath => "Auswählen",
        UiText::SelectPath => "Wählen",
        UiText::CurrentDirectory => "Aktuelles Verzeichnis:",
        UiText::ParentDirectory => "Übergeordnet",
        UiText::RefreshDirectory => "Aktualisieren",
        UiText::ShowHiddenDirectories => "Versteckte Verzeichnisse anzeigen",
        UiText::SelectedPath => "Ausgewählt:",
        UiText::OutputFileName => "Dateiname",
        UiText::SelectMorrowindIni => "Morrowind.ini auswählen",
        UiText::SelectExistingOpenmwCfg => "Vorhandene openmw.cfg auswählen",
        UiText::SelectOutputCfg => "Ausgabe-openmw.cfg auswählen",
        UiText::SelectGameDataDir => "Data-Files-Verzeichnis auswählen",
        UiText::SelectDataLocalDir => "data-local-Verzeichnis auswählen",
        UiText::SelectResourcesDir => "resources-Verzeichnis auswählen",
        UiText::SelectUserDataDir => "user-data-Verzeichnis auswählen",
        UiText::ControllerHelp => {
            "Controller: Steuerkreuz/linker Stick bewegt • A schaltet/wählt • B beendet • X leert ausgewählten Pfad • Start importiert, wenn bereit • links/rechts passt Optionen an • rechter Stick scrollt die generierte cfg"
        }
        UiText::PickerControllerHelp => {
            "Controller: Steuerkreuz/linker Stick bewegt • A/Enter öffnet oder wählt • B bricht ab • Links geht zum übergeordneten Verzeichnis • Rechts öffnet • Start wählt den aktuellen/erwarteten Pfad • LB schaltet versteckte Verzeichnisse um"
        }
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Leerer Wert für Schlüssel `{key}` ignoriert.")
        }
        ImportWarning::MalformedIniLine { line } => {
            format!("Fehlerhafte INI-Zeile ignoriert: {line}")
        }
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Inhaltsdatei aufgelöst: {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Archiv aufgelöst: {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!(
                "Datenverzeichnis für Inhaltsdateien hinzugefügt: {}",
                path.display()
            )
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Datenverzeichnis für Fallback-Archive hinzugefügt: {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!(
                "Konnte {} nicht lesen oder schreiben: {source}",
                path.display()
            )
        }
        ImportError::UnsupportedEncoding(value) => {
            format!("Nicht unterstützte Textkodierung: {value}")
        }
        ImportError::InvalidPluginHeader { path, message } => {
            format!("Ungültiger Plugin-Header in {}: {message}", path.display())
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Inhaltsdateien nicht gefunden: {}", files.join(", "))
        }
        ImportError::MissingArchives { files, .. } => {
            format!("Fallback-Archive nicht gefunden: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Ungültiger Inhaltsdateiname: {file}")
        }
        ImportError::InvalidArchiveName(file) => {
            format!("Ungültiger Fallback-Archivname: {file}")
        }
        _ => error.to_string(),
    }
}
