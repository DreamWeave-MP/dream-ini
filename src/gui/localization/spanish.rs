use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Idioma",
        UiText::EnglishLanguage => "Inglés",
        UiText::FrenchLanguage => "Francés",
        UiText::GermanLanguage => "Alemán",
        UiText::SpanishLanguage => "Español",
        UiText::SourceSection => "Fuente",
        UiText::Existing => "Existente",
        UiText::Browse => "Examinar…",
        UiText::ImportOptions => "Opciones de importación",
        UiText::Encoding => "Codificación",
        UiText::ImportFallbacks => "Importar fuentes bitmap",
        UiText::ImportArchives => "Importar archivos",
        UiText::ImportContentFiles => "Importar archivos de contenido / orden de carga",
        UiText::Overrides => "Sobrescrituras",
        UiText::ExplicitSearchPath => "Ruta de instalación del juego",
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
