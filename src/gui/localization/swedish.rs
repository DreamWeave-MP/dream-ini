// SPDX-License-Identifier: GPL-3.0-only

use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Språk",
        UiText::EnglishLanguage => "Engelska",
        UiText::FrenchLanguage => "Franska",
        UiText::GermanLanguage => "Tyska",
        UiText::RussianLanguage => "Ryska",
        UiText::SpanishLanguage => "Spanska",
        UiText::SwedishLanguage => "Svenska",
        UiText::SourceSection => "Källa",
        UiText::Existing => "Befintlig",
        UiText::Browse => "Bläddra…",
        UiText::ImportOptions => "Importalternativ",
        UiText::Encoding => "Kodning",
        UiText::EncodingAuto => "Auto",
        UiText::ImportFallbacks => "Importera bitmappsteckensnitt",
        UiText::ImportArchives => "Importera arkiv",
        UiText::ImportContentFiles => "Importera innehållsfiler / laddningsordning",
        UiText::Overrides => "Åsidosättningar",
        UiText::ExplicitSearchPath => "Data Files-katalog",
        UiText::Output => "Utdata",
        UiText::PreviewOnly => "Endast förhandsgranskning",
        UiText::SaveAs => "Spara som",
        UiText::OutputPath => "Utdataväg",
        UiText::UpdateExistingCfg => "Uppdatera befintlig openmw.cfg",
        UiText::ImportPreview => "Importera / Förhandsgranska",
        UiText::CannotImport => "Kan inte importera:",
        UiText::Results => "Resultat",
        UiText::Errors => "Fel",
        UiText::Warnings => "Varningar",
        UiText::Events => "Händelser",
        UiText::GeneratedCfg => "Genererad cfg",
        UiText::Copy => "Kopiera",
        UiText::Clear => "Rensa",
        UiText::EncodingTooltip => {
            "Teckenkodning som används när innehålls- och plugintext läses. Auto använder kodningen i befintlig cfg, eller win1252 om ingen är angiven."
        }
        UiText::ImportArchivesTooltip => {
            "Importerar fallback-archive-poster och hittar refererade .bsa-filer."
        }
        UiText::ImportContentFilesTooltip => {
            "Importerar GameFile-poster som innehållets laddningsordning och hittar refererade pluginfiler."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Valfri Morrowind Data Files-katalog som används för att hitta importerat innehåll och arkiv."
        }
        UiText::DataLocalTooltip => {
            "Skriver OpenMW:s runtime-inställning data-local. dream-ini söker inte i den här sökvägen under import; använd Data Files-katalogen för det."
        }
        UiText::ResourcesTooltip => {
            "Åsidosätter sökvägen till motorns resurser. Den bör peka på resurser från OpenMW; välj med omsorg."
        }
        UiText::UserDataTooltip => {
            "Åsidosätter var OpenMW lagrar användardata som sparfiler, skärmbilder och navmesh-cache."
        }
        UiText::NoErrors => "Inga fel.",
        UiText::NoWarnings => "Inga varningar.",
        UiText::NoEvents => "Inga händelser.",
        UiText::NoGeneratedCfg => "Ingen cfg genererades.",
        UiText::WroteCfgTo => "Skrev cfg till:",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Välj en Morrowind.ini-fil innan du importerar."
        }
        UiText::SelectOutputPathBeforeImporting => "Välj en utdataväg innan du importerar.",
        UiText::SelectExistingCfgBeforeUpdating => {
            "Välj en befintlig openmw.cfg innan du uppdaterar på plats."
        }
        UiText::CancelPicker => "Avbryt",
        UiText::ChoosePath | UiText::SelectPath => "Välj",
        UiText::CurrentDirectory => "Aktuell katalog:",
        UiText::ParentDirectory => "Överordnad",
        UiText::RefreshDirectory => "Uppdatera",
        UiText::ShowHiddenDirectories => "Visa dolda kataloger",
        UiText::SelectedPath => "Vald:",
        UiText::OutputFileName => "Filnamn",
        UiText::SelectMorrowindIni => "Välj Morrowind.ini",
        UiText::SelectExistingOpenmwCfg => "Välj befintlig openmw.cfg",
        UiText::SelectOutputCfg => "Välj utdata-openmw.cfg",
        UiText::SelectGameDataDir => "Välj Data Files-katalog",
        UiText::SelectDataLocalDir => "Välj data-local-katalog",
        UiText::SelectResourcesDir => "Välj resources-katalog",
        UiText::SelectUserDataDir => "Välj user-data-katalog",
        UiText::ControllerHelp => {
            "Handkontroll: styrkors/vänster spak flyttar • A växlar/väljer • B avslutar • X rensar vald sökväg • Start importerar när det går • vänster/höger justerar alternativ • höger spak rullar genererad cfg • LB/RB bläddrar i genererad cfg"
        }
        UiText::PickerControllerHelp => {
            "Handkontroll: styrkors/vänster spak flyttar • A/Enter öppnar eller väljer • B avbryter • Vänster går till överordnad katalog • Höger öppnar • Start väljer aktuell/förväntad sökväg • LB växlar dolda kataloger"
        }
        UiText::OskTitle => "Sökvägstangentbord",
        UiText::OskControllerHelp => {
            "Handkontroll: styrkors/vänster spak flyttar • A trycker tangent • B skiftar • Y mellanslag • X backsteg • Select/Escape avbryter • Start/OK använder"
        }
        UiText::OskOk => "OK",
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Ignorerade tomt värde för nyckeln `{key}`.")
        }
        ImportWarning::MalformedIniLine { line } => {
            format!("Ignorerade felaktig INI-rad: {line}")
        }
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Hittade innehållsfil: {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Hittade arkiv: {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!(
                "Lade till datakatalog för innehållsfiler: {}",
                path.display()
            )
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Lade till datakatalog för fallback-arkiv: {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!("Kunde inte läsa eller skriva {}: {source}", path.display())
        }
        ImportError::UnsupportedEncoding(value) => {
            format!("Textkodningen stöds inte: {value}")
        }
        ImportError::InvalidPluginHeader { path, message } => {
            format!("Ogiltig plugin-header i {}: {message}", path.display())
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Innehållsfiler hittades inte: {}", files.join(", "))
        }
        ImportError::MissingArchives { files, .. } => {
            format!("Fallback-arkiv hittades inte: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Ogiltigt namn på innehållsfil: {file}")
        }
        ImportError::InvalidArchiveName(file) => {
            format!("Ogiltigt namn på fallback-arkiv: {file}")
        }
        _ => error.to_string(),
    }
}
