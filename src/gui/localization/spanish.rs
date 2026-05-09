use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Idioma",
        UiText::EnglishLanguage => "Inglés",
        UiText::FrenchLanguage => "Francés",
        UiText::GermanLanguage => "Alemán",
        UiText::RussianLanguage => "Ruso",
        UiText::SpanishLanguage => "Español",
        UiText::SourceSection => "Fuente",
        UiText::Existing => "Existente",
        UiText::Browse => "Examinar…",
        UiText::ImportOptions => "Opciones de importación",
        UiText::Encoding => "Codificación",
        UiText::EncodingAuto => "Auto",
        UiText::ImportFallbacks => "Importar fuentes bitmap",
        UiText::ImportArchives => "Importar archivos",
        UiText::ImportContentFiles => "Importar archivos de contenido / orden de carga",
        UiText::Overrides => "Sobrescrituras",
        UiText::ExplicitSearchPath => "Directorio Data Files",
        UiText::Output => "Salida",
        UiText::PreviewOnly => "Solo vista previa",
        UiText::SaveAs => "Guardar como",
        UiText::OutputPath => "Ruta de salida",
        UiText::UpdateExistingCfg => "Actualizar openmw.cfg existente",
        UiText::ImportPreview => "Importar / Vista previa",
        UiText::CannotImport => "No se puede importar:",
        UiText::Results => "Resultados",
        UiText::Errors => "Errores",
        UiText::Warnings => "Advertencias",
        UiText::Events => "Eventos",
        UiText::GeneratedCfg => "Cfg generado",
        UiText::Copy => "Copiar",
        UiText::Clear => "Borrar",
        UiText::EncodingTooltip => {
            "Codificación de caracteres usada al leer texto de contenido y plugins. Auto usa la codificación del cfg existente, o win1252 si no hay ninguna definida."
        }
        UiText::ImportArchivesTooltip => {
            "Importa entradas fallback-archive y resuelve los archivos .bsa referenciados."
        }
        UiText::ImportContentFilesTooltip => {
            "Importa entradas GameFile como orden de carga y resuelve los plugins referenciados."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Directorio Data Files opcional usado para resolver contenido y archivos BSA importados."
        }
        UiText::DataLocalTooltip => {
            "Escribe la opción runtime data-local de OpenMW. dream-ini no busca en esta ruta al importar; use el directorio Data Files para eso."
        }
        UiText::ResourcesTooltip => {
            "Sobrescribe la ruta de recursos del motor. Debe apuntar a recursos proporcionados por OpenMW; elíjala con cuidado."
        }
        UiText::UserDataTooltip => {
            "Sobrescribe dónde OpenMW guarda datos de usuario como partidas, capturas de pantalla y caché de navmesh."
        }
        UiText::NoErrors => "Sin errores.",
        UiText::NoWarnings => "Sin advertencias.",
        UiText::NoEvents => "Sin eventos.",
        UiText::NoGeneratedCfg => "No se generó ningún cfg.",
        UiText::WroteCfgTo => "Cfg escrito en:",
        UiText::SelectMorrowindIniBeforeImporting => {
            "Seleccione un archivo Morrowind.ini antes de importar."
        }
        UiText::SelectOutputPathBeforeImporting => {
            "Seleccione una ruta de salida antes de importar."
        }
        UiText::SelectExistingCfgBeforeUpdating => {
            "Seleccione un openmw.cfg existente antes de actualizarlo directamente."
        }
        UiText::CancelPicker => "Cancelar",
        UiText::ChoosePath => "Elegir",
        UiText::SelectPath => "Seleccionar",
        UiText::CurrentDirectory => "Directorio actual:",
        UiText::ParentDirectory => "Superior",
        UiText::RefreshDirectory => "Actualizar",
        UiText::ShowHiddenDirectories => "Mostrar directorios ocultos",
        UiText::SelectedPath => "Seleccionado:",
        UiText::OutputFileName => "Nombre de archivo",
        UiText::SelectMorrowindIni => "Seleccionar Morrowind.ini",
        UiText::SelectExistingOpenmwCfg => "Seleccionar openmw.cfg existente",
        UiText::SelectOutputCfg => "Seleccionar openmw.cfg de salida",
        UiText::SelectGameDataDir => "Seleccionar directorio Data Files",
        UiText::SelectDataLocalDir => "Seleccionar directorio data-local",
        UiText::SelectResourcesDir => "Seleccionar directorio resources",
        UiText::SelectUserDataDir => "Seleccionar directorio user-data",
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Valor vacío ignorado para la clave `{key}`.")
        }
        ImportWarning::MalformedIniLine { line } => {
            format!("Línea INI mal formada ignorada: {line}")
        }
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Archivo de contenido resuelto: {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Archivo resuelto: {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!(
                "Directorio de datos añadido para archivos de contenido: {}",
                path.display()
            )
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Directorio de datos añadido para archivos fallback: {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!("No se pudo leer ni escribir {}: {source}", path.display())
        }
        ImportError::UnsupportedEncoding(value) => {
            format!("Codificación de texto no compatible: {value}")
        }
        ImportError::InvalidPluginHeader { path, message } => {
            format!(
                "Cabecera de plugin no válida en {}: {message}",
                path.display()
            )
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Archivos de contenido no encontrados: {}", files.join(", "))
        }
        ImportError::MissingArchives { files, .. } => {
            format!("Archivos fallback no encontrados: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Nombre de archivo de contenido no válido: {file}")
        }
        ImportError::InvalidArchiveName(file) => {
            format!("Nombre de archivo fallback no válido: {file}")
        }
        _ => error.to_string(),
    }
}
