// SPDX-License-Identifier: GPL-3.0-only

use dream_ini::{ImportError, ImportEvent, ImportWarning};

use super::UiText;

pub(super) const fn text(key: UiText) -> &'static str {
    match key {
        UiText::Language => "Язык",
        UiText::EnglishLanguage => "Английский",
        UiText::FrenchLanguage => "Французский",
        UiText::GermanLanguage => "Немецкий",
        UiText::RussianLanguage => "Русский",
        UiText::SpanishLanguage => "Испанский",
        UiText::SwedishLanguage => "Шведский",
        UiText::SourceSection => "Исходные файлы",
        UiText::Existing => "Существующий",
        UiText::Browse => "Обзор…",
        UiText::ImportOptions => "Параметры импорта",
        UiText::Encoding => "Кодировка",
        UiText::EncodingAuto => "Авто",
        UiText::ImportFallbacks => "Импорт bitmap-шрифтов",
        UiText::ImportArchives => "Импорт архивов",
        UiText::ImportContentFiles => "Импорт контента / порядок загрузки",
        UiText::Overrides => "Переопределения",
        UiText::ExplicitSearchPath => "Каталог Data Files",
        UiText::Output => "Вывод",
        UiText::PreviewOnly => "Только предпросмотр",
        UiText::SaveAs => "Сохранить как",
        UiText::OutputPath => "Путь вывода",
        UiText::UpdateExistingCfg => "Обновить существующий openmw.cfg",
        UiText::ImportPreview => "Импорт / предпросмотр",
        UiText::CannotImport => "Невозможно импортировать:",
        UiText::Results => "Результаты",
        UiText::Errors => "Ошибки",
        UiText::Warnings => "Предупреждения",
        UiText::Events => "События",
        UiText::GeneratedCfg => "Сгенерированный cfg",
        UiText::Copy => "Копировать",
        UiText::Clear => "Очистить",
        UiText::EncodingTooltip => {
            "Кодировка для чтения текста контента и плагинов. Авто использует кодировку из существующего cfg, а если она не задана — win1252."
        }
        UiText::ImportArchivesTooltip => {
            "Импортирует записи fallback-archive и находит указанные файлы .bsa."
        }
        UiText::ImportContentFilesTooltip => {
            "Импортирует записи GameFile как порядок загрузки и находит указанные плагины."
        }
        UiText::ExplicitSearchPathTooltip => {
            "Необязательный каталог Morrowind Data Files для поиска импортируемого контента и архивов BSA."
        }
        UiText::DataLocalTooltip => {
            "Записывает runtime-настройку OpenMW data-local. dream-ini не ищет в этом пути при импорте; для этого используйте каталог Data Files."
        }
        UiText::ResourcesTooltip => {
            "Переопределяет путь к ресурсам движка. Он должен указывать на ресурсы, поставляемые OpenMW; выбирайте осторожно."
        }
        UiText::UserDataTooltip => {
            "Переопределяет место, где OpenMW хранит пользовательские данные: сохранения, скриншоты и кэш navmesh."
        }
        UiText::NoErrors => "Ошибок нет.",
        UiText::NoWarnings => "Предупреждений нет.",
        UiText::NoEvents => "Событий нет.",
        UiText::NoGeneratedCfg => "Сгенерированного cfg нет.",
        UiText::WroteCfgTo => "Cfg записан в:",
        UiText::SelectMorrowindIniBeforeImporting => "Выберите файл Morrowind.ini перед импортом.",
        UiText::SelectOutputPathBeforeImporting => "Выберите путь вывода перед импортом.",
        UiText::SelectExistingCfgBeforeUpdating => {
            "Выберите существующий openmw.cfg перед обновлением на месте."
        }
        UiText::CancelPicker => "Отмена",
        UiText::ChoosePath | UiText::SelectPath => "Выбрать",
        UiText::CurrentDirectory => "Текущий каталог:",
        UiText::ParentDirectory => "На уровень выше",
        UiText::RefreshDirectory => "Обновить",
        UiText::ShowHiddenDirectories => "Показывать скрытые каталоги",
        UiText::SelectedPath => "Выбрано:",
        UiText::OutputFileName => "Имя файла",
        UiText::SelectMorrowindIni => "Выберите Morrowind.ini",
        UiText::SelectExistingOpenmwCfg => "Выберите существующий openmw.cfg",
        UiText::SelectOutputCfg => "Выберите выходной openmw.cfg",
        UiText::SelectGameDataDir => "Выберите каталог Data Files",
        UiText::SelectDataLocalDir => "Выберите каталог data-local",
        UiText::SelectResourcesDir => "Выберите каталог resources",
        UiText::SelectUserDataDir => "Выберите каталог пользовательских данных",
        UiText::ControllerHelp => {
            "Контроллер: D-pad/левый стик — перемещение • A — переключить/выбрать • B — выйти • X — очистить выбранный путь • Start — импорт, когда всё готово • влево/вправо — изменить параметры • правый стик — прокрутка cfg • LB/RB — страницы generated cfg"
        }
        UiText::PickerControllerHelp => {
            "Контроллер: D-pad/левый стик — перемещение • A/Enter — открыть или выбрать • B — отмена • Влево — родительский каталог • Вправо — войти • Start — выбрать текущий/ожидаемый путь • LB — переключить скрытые каталоги"
        }
        UiText::OskTitle => "Клавиатура пути",
        UiText::OskControllerHelp => {
            "Контроллер: D-pad/левый стик — перемещение • A — нажать клавишу • B — Shift • Y — пробел • X — удалить символ • Select/Escape — отмена • Start/OK — применить"
        }
        UiText::OskOk => "OK",
    }
}

pub(super) fn warning_title(warning: &ImportWarning) -> String {
    match warning {
        ImportWarning::IgnoredEmptyValue { key } => {
            format!("Пустое значение ключа `{key}` проигнорировано.")
        }
        ImportWarning::MalformedIniLine { line } => {
            format!("Некорректная строка INI проигнорирована: {line}")
        }
    }
}

pub(super) fn event_title(event: &ImportEvent) -> String {
    match event {
        ImportEvent::ContentFileResolved { path, .. } => {
            format!("Файл контента найден: {}", path.display())
        }
        ImportEvent::ArchiveResolved { path } => format!("Архив найден: {}", path.display()),
        ImportEvent::DataDirAddedForContent { path } => {
            format!(
                "Добавлен каталог данных для файлов контента: {}",
                path.display()
            )
        }
        ImportEvent::DataDirAddedForArchive { path } => {
            format!(
                "Добавлен каталог данных для fallback-архивов: {}",
                path.display()
            )
        }
    }
}

pub(super) fn error_title(error: &ImportError) -> String {
    match error {
        ImportError::Io { path, source } => {
            format!(
                "Не удалось прочитать или записать {}: {source}",
                path.display()
            )
        }
        ImportError::UnsupportedEncoding(value) => {
            format!("Неподдерживаемая текстовая кодировка: {value}")
        }
        ImportError::InvalidPluginHeader { path, message } => {
            format!(
                "Некорректный заголовок плагина в {}: {message}",
                path.display()
            )
        }
        ImportError::MissingContentFiles { files, .. } => {
            format!("Файлы контента не найдены: {}", files.join(", "))
        }
        ImportError::MissingArchives { files, .. } => {
            format!("Fallback-архивы не найдены: {}", files.join(", "))
        }
        ImportError::InvalidContentFileName(file) => {
            format!("Некорректное имя файла контента: {file}")
        }
        ImportError::InvalidArchiveName(file) => {
            format!("Некорректное имя fallback-архива: {file}")
        }
        _ => error.to_string(),
    }
}
