use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

use eframe::egui;

use super::localization::{Localizer, UiText};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PathTarget {
    MorrowindIni,
    ExistingOpenmwCfg,
    OutputCfg,
    GameDataDir,
    DataLocalDir,
    ResourcesDir,
    UserDataDir,
}

#[derive(Debug)]
pub(super) enum PickOutcome {
    None,
    Cancelled,
    Chosen { target: PathTarget, path: PathBuf },
}

#[derive(Debug)]
pub(super) struct PathPickerState {
    target: PathTarget,
    current_dir: PathBuf,
    selected: Option<PathBuf>,
    entries: Vec<PathEntry>,
    error: Option<String>,
    output_file_name: String,
    current_dir_readable: bool,
    show_hidden_directories: bool,
}

impl PathPickerState {
    pub(super) fn new(target: PathTarget, initial_path: Option<&Path>) -> Self {
        let current_dir = initial_path
            .and_then(initial_directory)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let selected = if target.is_directory_target() || target == PathTarget::OutputCfg {
            Some(current_dir.clone())
        } else {
            initial_path
                .filter(|path| path.is_file())
                .map(Path::to_path_buf)
        };
        let output_file_name = initial_path
            .filter(|_| target == PathTarget::OutputCfg)
            .and_then(Path::file_name)
            .and_then(OsStr::to_str)
            .filter(|file_name| !file_name.is_empty())
            .unwrap_or("openmw.cfg")
            .to_owned();

        let mut state = Self {
            target,
            current_dir,
            selected,
            entries: Vec::new(),
            error: None,
            output_file_name,
            current_dir_readable: false,
            show_hidden_directories: false,
        };
        state.refresh();
        state
    }

    pub(super) fn ui(&mut self, ui: &mut egui::Ui, localizer: Localizer) -> PickOutcome {
        if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
            return PickOutcome::Cancelled;
        }

        let mut outcome = PickOutcome::None;
        ui.horizontal(|ui| {
            if ui.button(localizer.text(UiText::CancelPicker)).clicked() {
                outcome = PickOutcome::Cancelled;
            }
            ui.heading(self.title(localizer));
        });
        ui.separator();

        ui.label(localizer.text(UiText::CurrentDirectory));
        ui.monospace(self.current_dir.display().to_string());
        ui.horizontal(|ui| {
            if ui.button(localizer.text(UiText::ParentDirectory)).clicked() {
                self.enter_parent();
            }
            if ui
                .button(localizer.text(UiText::RefreshDirectory))
                .clicked()
            {
                self.refresh();
            }
            if self.target.is_directory_target()
                && ui.button(localizer.text(UiText::SelectPath)).clicked()
            {
                outcome = PickOutcome::Chosen {
                    target: self.target,
                    path: self.current_dir.clone(),
                };
            }
        });

        if ui
            .checkbox(
                &mut self.show_hidden_directories,
                localizer.text(UiText::ShowHiddenDirectories),
            )
            .changed()
        {
            self.refresh();
        }
        if let Some(error) = &self.error {
            ui.colored_label(egui::Color32::RED, error);
        }

        ui.separator();
        let mut entry_action = EntryAction::None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for entry in &self.entries {
                    entry_action = self.entry_row(ui, entry);
                    if !matches!(entry_action, EntryAction::None) {
                        break;
                    }
                }
            });
        match entry_action {
            EntryAction::None => {}
            EntryAction::Navigate(path) => self.enter_directory(path),
            EntryAction::SelectFile(path) => self.select_file(path),
            EntryAction::Choose(path) => {
                outcome = PickOutcome::Chosen {
                    target: self.target,
                    path,
                };
            }
        }

        ui.separator();
        if self.target == PathTarget::OutputCfg {
            ui.horizontal(|ui| {
                ui.label(localizer.text(UiText::OutputFileName));
                ui.text_edit_singleline(&mut self.output_file_name);
            });
        }

        let chosen_path = self.chosen_path();
        ui.label(localizer.text(UiText::SelectedPath));
        ui.monospace(
            chosen_path
                .as_ref()
                .map_or_else(String::new, |path| path.display().to_string()),
        );

        if !self.target.is_directory_target() {
            let choose_enabled = chosen_path.is_some();
            let enter_pressed = ui.input(|input| input.key_pressed(egui::Key::Enter));
            if (ui
                .add_enabled(
                    choose_enabled,
                    egui::Button::new(localizer.text(UiText::ChoosePath)),
                )
                .clicked()
                || (enter_pressed && choose_enabled))
                && let Some(path) = chosen_path
            {
                outcome = PickOutcome::Chosen {
                    target: self.target,
                    path,
                };
            }
        }

        outcome
    }

    fn title(&self, localizer: Localizer) -> &'static str {
        match self.target {
            PathTarget::MorrowindIni => localizer.text(UiText::SelectMorrowindIni),
            PathTarget::ExistingOpenmwCfg => localizer.text(UiText::SelectExistingOpenmwCfg),
            PathTarget::OutputCfg => localizer.text(UiText::SelectOutputCfg),
            PathTarget::GameDataDir => localizer.text(UiText::SelectGameDataDir),
            PathTarget::DataLocalDir => localizer.text(UiText::SelectDataLocalDir),
            PathTarget::ResourcesDir => localizer.text(UiText::SelectResourcesDir),
            PathTarget::UserDataDir => localizer.text(UiText::SelectUserDataDir),
        }
    }

    fn refresh(&mut self) {
        match read_entries(self.target, &self.current_dir, self.show_hidden_directories) {
            Ok(ReadEntries {
                entries,
                skipped_entries,
            }) => {
                self.entries = entries;
                self.current_dir_readable = true;
                self.revalidate_selection();
                self.error = skipped_entries_message(&skipped_entries);
            }
            Err(error) => {
                self.entries.clear();
                self.selected = None;
                self.current_dir_readable = false;
                self.error = Some(format!(
                    "Could not read directory {}: {error}",
                    self.current_dir.display()
                ));
            }
        }
    }

    fn enter_parent(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            self.enter_directory(parent.to_path_buf());
        }
    }

    fn enter_directory(&mut self, path: PathBuf) {
        self.current_dir = path;
        self.selected = if self.target.is_directory_target() {
            Some(self.current_dir.clone())
        } else {
            None
        };
        self.refresh();
    }

    fn entry_row(&self, ui: &mut egui::Ui, entry: &PathEntry) -> EntryAction {
        let label = match entry.kind {
            EntryKind::Parent => format!("↑ {}", entry.name),
            EntryKind::Directory => format!("📁 {}", entry.name),
            EntryKind::File => format!("📄 {}", entry.name),
        };
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|path| path == &entry.path);
        let response = ui.selectable_label(selected, label);
        if response.double_clicked() {
            return match entry.kind {
                EntryKind::Parent | EntryKind::Directory => {
                    EntryAction::Navigate(entry.path.clone())
                }
                EntryKind::File => EntryAction::Choose(entry.path.clone()),
            };
        }
        if response.clicked() {
            return match entry.kind {
                EntryKind::Parent | EntryKind::Directory => {
                    EntryAction::Navigate(entry.path.clone())
                }
                EntryKind::File => EntryAction::SelectFile(entry.path.clone()),
            };
        }
        EntryAction::None
    }

    fn select_file(&mut self, path: PathBuf) {
        if self.target == PathTarget::OutputCfg {
            if let Some(file_name) = path.file_name().and_then(OsStr::to_str) {
                file_name.clone_into(&mut self.output_file_name);
            }
            self.selected = path.parent().map(Path::to_path_buf);
        } else {
            self.selected = Some(path);
        }
    }

    fn chosen_path(&self) -> Option<PathBuf> {
        if self.target == PathTarget::OutputCfg {
            let file_name = self.output_file_name.trim();
            if !self.current_dir_readable || !valid_output_file_name(file_name) {
                return None;
            }
            let directory = self.selected.as_deref().unwrap_or(&self.current_dir);
            return Some(directory.join(file_name));
        }
        self.selected.clone()
    }

    fn revalidate_selection(&mut self) {
        match self.target.pick_kind() {
            PickKind::Directory => {
                if self.selected.as_ref().is_none_or(|path| {
                    path != &self.current_dir && !self.entry_exists(path, EntryKind::Directory)
                }) {
                    self.selected = Some(self.current_dir.clone());
                }
            }
            PickKind::ExistingFile => {
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|path| !self.entry_exists(path, EntryKind::File))
                {
                    self.selected = None;
                }
            }
            PickKind::OutputCfg => {
                if self.selected.as_ref().is_none_or(|path| {
                    path != &self.current_dir && !self.entry_exists(path, EntryKind::Directory)
                }) {
                    self.selected = Some(self.current_dir.clone());
                }
            }
        }
    }

    fn entry_exists(&self, path: &Path, kind: EntryKind) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.kind == kind && entry.path == path)
    }
}

impl PathTarget {
    const fn is_directory_target(self) -> bool {
        matches!(
            self,
            Self::GameDataDir | Self::DataLocalDir | Self::ResourcesDir | Self::UserDataDir
        )
    }

    const fn pick_kind(self) -> PickKind {
        match self {
            Self::MorrowindIni | Self::ExistingOpenmwCfg => PickKind::ExistingFile,
            Self::OutputCfg => PickKind::OutputCfg,
            Self::GameDataDir | Self::DataLocalDir | Self::ResourcesDir | Self::UserDataDir => {
                PickKind::Directory
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PathEntry {
    name: String,
    path: PathBuf,
    kind: EntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryKind {
    Parent,
    Directory,
    File,
}

enum EntryAction {
    None,
    Navigate(PathBuf),
    SelectFile(PathBuf),
    Choose(PathBuf),
}

#[derive(Debug, Clone, Copy)]
enum PickKind {
    Directory,
    ExistingFile,
    OutputCfg,
}

fn initial_directory(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return Some(path.to_path_buf());
    }
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

struct ReadEntries {
    entries: Vec<PathEntry>,
    skipped_entries: Vec<String>,
}

fn read_entries(
    target: PathTarget,
    directory: &Path,
    show_hidden_directories: bool,
) -> std::io::Result<ReadEntries> {
    let mut directories = Vec::new();
    let mut files = Vec::new();
    let mut skipped_entries = Vec::new();

    if let Some(parent) = directory.parent() {
        directories.push(PathEntry {
            name: "..".to_owned(),
            path: parent.to_path_buf(),
            kind: EntryKind::Parent,
        });
    }

    for entry in fs::read_dir(directory)? {
        let Ok(entry) = entry else {
            skipped_entries.push("<unreadable entry>".to_owned());
            continue;
        };
        let path = entry.path();
        let Ok(metadata) = fs::metadata(&path) else {
            skipped_entries.push(entry.file_name().to_string_lossy().into_owned());
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        if metadata.is_dir() && (show_hidden_directories || !is_hidden_name(&name)) {
            directories.push(PathEntry {
                name,
                path,
                kind: EntryKind::Directory,
            });
        } else if metadata.is_file() && target.displays_file_name(&entry.file_name()) {
            files.push(PathEntry {
                name,
                path,
                kind: EntryKind::File,
            });
        }
    }

    let sort_start = usize::from(directory.parent().is_some());
    directories[sort_start..].sort_by(|left, right| compare_names(&left.name, &right.name));
    files.sort_by(|left, right| compare_names(&left.name, &right.name));
    directories.extend(files);
    Ok(ReadEntries {
        entries: directories,
        skipped_entries,
    })
}

fn skipped_entries_message(skipped_entries: &[String]) -> Option<String> {
    if skipped_entries.is_empty() {
        return None;
    }
    let visible_names = skipped_entries
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let remaining_count = skipped_entries.len().saturating_sub(3);
    let suffix = if remaining_count == 0 {
        String::new()
    } else {
        format!(" (+{remaining_count} more)")
    };
    Some(format!(
        "Skipped {} unreadable directory entries: {visible_names}{suffix}",
        skipped_entries.len()
    ))
}

fn valid_output_file_name(file_name: &str) -> bool {
    let mut components = Path::new(file_name).components();
    matches!(components.next(), Some(Component::Normal(_))) && components.next().is_none()
}

fn is_hidden_name(file_name: &str) -> bool {
    file_name.starts_with('.') && file_name != "." && file_name != ".."
}

impl PathTarget {
    fn displays_file_name(self, file_name: &OsStr) -> bool {
        self.expected_file_name()
            .is_some_and(|expected_file_name| file_name == OsStr::new(expected_file_name))
    }

    const fn expected_file_name(self) -> Option<&'static str> {
        match self {
            Self::MorrowindIni => Some("Morrowind.ini"),
            Self::ExistingOpenmwCfg | Self::OutputCfg => Some("openmw.cfg"),
            Self::GameDataDir | Self::DataLocalDir | Self::ResourcesDir | Self::UserDataDir => None,
        }
    }
}

fn compare_names(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase()
        .cmp(&right.to_lowercase())
        .then_with(|| left.cmp(right))
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};

    use super::*;

    #[test]
    fn lists_directories_and_expected_file_for_file_targets() {
        let root = unique_temp_dir();
        fs::create_dir(root.join("Data Files")).unwrap();
        fs::create_dir(root.join("Saves")).unwrap();
        File::create(root.join("Morrowind.ini")).unwrap();
        File::create(root.join("openmw.cfg")).unwrap();
        File::create(root.join("notes.txt")).unwrap();

        let entries = read_entries(PathTarget::MorrowindIni, &root, false)
            .unwrap()
            .entries;
        let names: Vec<_> = entries.into_iter().map(|entry| entry.name).collect();

        assert!(names.contains(&"Data Files".to_owned()));
        assert!(names.contains(&"Saves".to_owned()));
        assert!(names.contains(&"Morrowind.ini".to_owned()));
        assert!(!names.contains(&"openmw.cfg".to_owned()));
        assert!(!names.contains(&"notes.txt".to_owned()));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn directory_targets_hide_files() {
        let root = unique_temp_dir();
        fs::create_dir(root.join("user-data")).unwrap();
        File::create(root.join("openmw.cfg")).unwrap();

        let entries = read_entries(PathTarget::UserDataDir, &root, false)
            .unwrap()
            .entries;
        let names: Vec<_> = entries.into_iter().map(|entry| entry.name).collect();

        assert!(names.contains(&"user-data".to_owned()));
        assert!(!names.contains(&"openmw.cfg".to_owned()));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hidden_directories_are_optionally_shown() {
        let root = unique_temp_dir();
        fs::create_dir(root.join(".hidden-dir")).unwrap();
        fs::create_dir(root.join("visible-dir")).unwrap();
        File::create(root.join(".hidden.cfg")).unwrap();
        File::create(root.join("openmw.cfg")).unwrap();

        let hidden_disabled = read_entries(PathTarget::ExistingOpenmwCfg, &root, false)
            .unwrap()
            .entries;
        let hidden_enabled = read_entries(PathTarget::ExistingOpenmwCfg, &root, true)
            .unwrap()
            .entries;
        let disabled_names: Vec<_> = hidden_disabled
            .into_iter()
            .map(|entry| entry.name)
            .collect();
        let enabled_names: Vec<_> = hidden_enabled.into_iter().map(|entry| entry.name).collect();

        assert!(disabled_names.contains(&"visible-dir".to_owned()));
        assert!(disabled_names.contains(&"openmw.cfg".to_owned()));
        assert!(!disabled_names.contains(&".hidden-dir".to_owned()));
        assert!(!disabled_names.contains(&".hidden.cfg".to_owned()));
        assert!(enabled_names.contains(&".hidden-dir".to_owned()));
        assert!(!enabled_names.contains(&".hidden.cfg".to_owned()));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_non_file_output_names() {
        assert!(valid_output_file_name("openmw.cfg"));
        assert!(!valid_output_file_name("."));
        assert!(!valid_output_file_name(".."));
        assert!(!valid_output_file_name("nested/openmw.cfg"));
        assert!(!valid_output_file_name(""));
    }

    #[test]
    fn refresh_clears_stale_file_selection() {
        let root = unique_temp_dir();
        let cfg = root.join("openmw.cfg");
        File::create(&cfg).unwrap();
        let mut picker = PathPickerState::new(PathTarget::ExistingOpenmwCfg, Some(&cfg));

        fs::remove_file(&cfg).unwrap();
        picker.refresh();

        assert!(picker.chosen_path().is_none());

        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn follows_symlinks_for_picker_entries() {
        let root = unique_temp_dir();
        fs::create_dir(root.join("RealDir")).unwrap();
        File::create(root.join("Real.ini")).unwrap();
        std::os::unix::fs::symlink(root.join("RealDir"), root.join("LinkedDir")).unwrap();
        std::os::unix::fs::symlink(root.join("Real.ini"), root.join("Morrowind.ini")).unwrap();

        let directory_entries = read_entries(PathTarget::UserDataDir, &root, false)
            .unwrap()
            .entries;
        let file_entries = read_entries(PathTarget::MorrowindIni, &root, false)
            .unwrap()
            .entries;

        assert!(
            directory_entries
                .iter()
                .any(|entry| entry.name == "LinkedDir" && entry.kind == EntryKind::Directory)
        );
        assert!(
            file_entries
                .iter()
                .any(|entry| entry.name == "Morrowind.ini" && entry.kind == EntryKind::File)
        );

        fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "dream-ini-picker-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        root
    }
}
