use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Langue",
        UiText::EnglishLanguage => "Anglais",
        UiText::FrenchLanguage => "Français",
        UiText::GermanLanguage => "Allemand",
        UiText::SpanishLanguage => "Espagnol",
        UiText::SourceSection => "Fichiers d’entrée",
        UiText::Existing => "Existant",
        UiText::Browse => "Parcourir…",
        UiText::ImportOptions => "Options d’importation",
        UiText::Encoding => "Encodage",
        UiText::ImportFallbacks => "Importer les polices bitmap",
        UiText::ImportArchives => "Importer les archives",
        UiText::ImportContentFiles => "Importer les fichiers de contenu / ordre de chargement",
        UiText::Overrides => "Remplacements",
        UiText::ExplicitSearchPath => "Chemin d’installation du jeu",
        UiText::Output => "Sortie",
        UiText::PreviewOnly => "Aperçu uniquement",
        UiText::SaveAs => "Enregistrer sous",
        UiText::OutputPath => "Chemin de sortie",
        UiText::UpdateExistingCfg => "Mettre à jour l’openmw.cfg existant",
        UiText::ImportPreview => "Importer / Aperçu",
        UiText::CannotImport => "Impossible d’importer :",
        UiText::Results => "Résultats",
        UiText::Errors => "Erreurs",
        UiText::Warnings => "Avertissements",
        UiText::Events => "Événements",
        UiText::GeneratedCfg => "Cfg généré",
        UiText::Copy => "Copier",
        UiText::Clear => "Effacer",
        UiText::EncodingTooltip => "Encodage des caractères utilisé pour lire Morrowind.ini.",
        UiText::ImportArchivesTooltip => {
            "Importe les entrées fallback-archive et résout les fichiers .bsa référencés."
        }
        UiText::ImportContentFilesTooltip => {
            "Importe les entrées GameFile comme ordre de chargement et résout les plugins référencés."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Racine optionnelle d’installation/Data Files utilisée pour trouver contenu et archives."
        }
        UiText::DataLocalTooltip => {
            "Remplace le chemin data-local d’OpenMW. Ce chemin de recherche est prioritaire."
        }
        UiText::ResourcesTooltip => "Remplace le chemin resources écrit dans openmw.cfg.",
        UiText::UserdataTooltip => "Remplace le chemin userdata écrit dans openmw.cfg.",
        UiText::NoErrors => "Aucune erreur.",
        UiText::NoWarnings => "Aucun avertissement.",
        UiText::NoEvents => "Aucun événement.",
        UiText::NoGeneratedCfg => "Aucun cfg généré.",
        UiText::WroteCfgTo => "Cfg écrit dans :",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Sélectionnez un fichier Morrowind.ini avant d’importer."
        }
        UiText::SelectOutputPathBeforeImporting => {
            "Sélectionnez un chemin de sortie avant d’importer."
        }
        UiText::SelectExistingCfgBeforeUpdating => {
            "Sélectionnez un openmw.cfg existant avant de le mettre à jour sur place."
        }
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Valeur vide ignorée pour la clé `{key}`.")
        }
        ImportWarning::MalformedIniLine { line } => {
            format!("Ligne INI mal formée ignorée : {line}")
        }
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Fichier de contenu résolu : {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Archive résolue : {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!(
                "Répertoire de données ajouté pour les fichiers de contenu : {}",
                path.display()
            )
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Répertoire de données ajouté pour les archives fallback : {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!(
                "Impossible de lire ou d’écrire {} : {source}",
                path.display()
            )
        }
        ImportError::UnsupportedEncoding(value) => {
            format!("Encodage de texte non pris en charge : {value}")
        }
        ImportError::InvalidPluginHeader { path, message } => {
            format!(
                "En-tête de plugin invalide dans {} : {message}",
                path.display()
            )
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Fichiers de contenu introuvables : {}", files.join(", "))
        }
        ImportError::MissingArchives { files, .. } => {
            format!("Archives fallback introuvables : {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Nom de fichier de contenu invalide : {file}")
        }
        ImportError::InvalidArchiveName(file) => {
            format!("Nom d’archive fallback invalide : {file}")
        }
        _ => error.to_string(),
    }
}
