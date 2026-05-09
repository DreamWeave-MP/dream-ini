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
        UiText::EncodingAuto => "Auto",
        UiText::ImportFallbacks => "Importer les polices bitmap",
        UiText::ImportArchives => "Importer les archives",
        UiText::ImportContentFiles => "Importer les fichiers de contenu / ordre de chargement",
        UiText::Overrides => "Remplacements",
        UiText::ExplicitSearchPath => "Répertoire Data Files",
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
        UiText::EncodingTooltip => {
            "Encodage des caractères utilisé pour lire le texte du contenu et des plugins. Auto utilise l’encodage du cfg existant, ou win1252 s’il n’est pas défini."
        }
        UiText::ImportArchivesTooltip => {
            "Importe les entrées fallback-archive et résout les fichiers .bsa référencés."
        }
        UiText::ImportContentFilesTooltip => {
            "Importe les entrées GameFile comme ordre de chargement et résout les plugins référencés."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Répertoire Data Files optionnel utilisé pour trouver le contenu et les archives importés."
        }
        UiText::DataLocalTooltip => {
            "Écrit le réglage runtime data-local d’OpenMW. dream-ini ne cherche pas dans ce chemin pendant l’importation ; utilisez le répertoire Data Files pour cela."
        }
        UiText::ResourcesTooltip => {
            "Remplace le chemin des ressources moteur. Il doit pointer vers les ressources fournies par OpenMW ; à choisir avec soin."
        }
        UiText::UserDataTooltip => {
            "Remplace l’emplacement où OpenMW stocke les données utilisateur : sauvegardes, captures d’écran et cache navmesh."
        }
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
        UiText::CancelPicker => "Annuler",
        UiText::ChoosePath => "Choisir",
        UiText::SelectPath => "Sélectionner",
        UiText::CurrentDirectory => "Répertoire actuel :",
        UiText::ParentDirectory => "Parent",
        UiText::RefreshDirectory => "Actualiser",
        UiText::ShowHiddenDirectories => "Afficher les répertoires cachés",
        UiText::SelectedPath => "Sélection :",
        UiText::OutputFileName => "Nom du fichier",
        UiText::SelectMorrowindIni => "Sélectionner Morrowind.ini",
        UiText::SelectExistingOpenmwCfg => "Sélectionner l’openmw.cfg existant",
        UiText::SelectOutputCfg => "Sélectionner l’openmw.cfg de sortie",
        UiText::SelectGameDataDir => "Sélectionner le répertoire Data Files",
        UiText::SelectDataLocalDir => "Sélectionner le répertoire data-local",
        UiText::SelectResourcesDir => "Sélectionner le répertoire resources",
        UiText::SelectUserDataDir => "Sélectionner le répertoire user-data",
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
