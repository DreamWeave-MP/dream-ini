// SPDX-License-Identifier: GPL-3.0-only

use std::env;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) const APP_ID: &str = "io.github.DreamWeave-MP.dream-ini";
pub(crate) const APP_NAME: &str = "Dream INI";
#[cfg(any(target_os = "linux", windows, test))]
const APP_COMMENT: &str = "Import Morrowind.ini settings into OpenMW configuration files";
#[cfg(target_os = "linux")]
const PNG_ICON_BYTES: &[u8] = include_bytes!("../assets/logo.png");
#[cfg(windows)]
const ICO_ICON_BYTES: &[u8] = include_bytes!("../assets/logo.ico");

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct LauncherInstallPaths {
    pub(crate) launcher: PathBuf,
    pub(crate) icon: PathBuf,
}

pub(crate) fn install(data_home: Option<&Path>) -> io::Result<LauncherInstallPaths> {
    install_with_executable(data_home, &env::current_exe()?)
}

#[cfg(target_os = "linux")]
fn install_with_executable(
    data_home: Option<&Path>,
    executable: &Path,
) -> io::Result<LauncherInstallPaths> {
    let data_home = data_home
        .map(Path::to_owned)
        .map_or_else(xdg_data_home, Ok)?;
    let paths = linux_install_paths(&data_home);

    if let Some(parent) = paths.launcher.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = paths.icon.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&paths.launcher, desktop_entry(executable))?;
    std::fs::write(&paths.icon, PNG_ICON_BYTES)?;

    Ok(paths)
}

#[cfg(windows)]
fn install_with_executable(
    data_home: Option<&Path>,
    executable: &Path,
) -> io::Result<LauncherInstallPaths> {
    let app_data = data_home
        .map(Path::to_owned)
        .map_or_else(windows_app_data, Ok)?;
    let paths = windows_install_paths(&app_data);

    if let Some(parent) = paths.launcher.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = paths.icon.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&paths.icon, ICO_ICON_BYTES)?;
    create_windows_shortcut(executable, &paths.icon, &paths.launcher)?;

    Ok(paths)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn install_with_executable(
    _data_home: Option<&Path>,
    _executable: &Path,
) -> io::Result<LauncherInstallPaths> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "launcher installation is not implemented on this platform",
    ))
}

#[cfg(target_os = "linux")]
fn linux_install_paths(data_home: &Path) -> LauncherInstallPaths {
    LauncherInstallPaths {
        launcher: data_home
            .join("applications")
            .join(format!("{APP_ID}.desktop")),
        icon: data_home
            .join("icons")
            .join("hicolor")
            .join("512x512")
            .join("apps")
            .join(format!("{APP_ID}.png")),
    }
}

#[cfg(windows)]
fn windows_install_paths(app_data: &Path) -> LauncherInstallPaths {
    LauncherInstallPaths {
        launcher: app_data
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join(format!("{APP_NAME}.lnk")),
        icon: app_data.join(APP_NAME).join("logo.ico"),
    }
}

#[cfg(windows)]
fn windows_app_data() -> io::Result<PathBuf> {
    env::var_os("APPDATA").map(PathBuf::from).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "APPDATA is not set; cannot locate the Start Menu for this user",
        )
    })
}

#[cfg(windows)]
fn create_windows_shortcut(executable: &Path, icon: &Path, shortcut: &Path) -> io::Result<()> {
    windows_shortcut::create(executable, icon, shortcut).map_err(io::Error::other)
}

#[cfg(target_os = "linux")]
fn xdg_data_home() -> io::Result<PathBuf> {
    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        let data_home = PathBuf::from(data_home);
        if data_home.is_absolute() {
            return Ok(data_home);
        }
    }

    let home = env::var_os("HOME").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "HOME is not set and XDG_DATA_HOME is not absolute",
        )
    })?;
    Ok(PathBuf::from(home).join(".local/share"))
}

#[cfg(any(target_os = "linux", test))]
pub(crate) fn desktop_entry(executable: &Path) -> String {
    let executable = desktop_exec_path(executable);
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name={APP_NAME}\n\
         Comment={APP_COMMENT}\n\
         Exec={executable}\n\
         Icon={APP_ID}\n\
         Terminal=false\n\
         Categories=Game;Utility;\n"
    )
}

#[cfg(windows)]
mod windows_shortcut {
    use std::path::Path;

    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};

    use super::APP_COMMENT;

    pub(super) fn create(
        executable: &Path,
        icon: &Path,
        shortcut: &Path,
    ) -> windows::core::Result<()> {
        let _com = ComApartment::initialize()?;

        // SAFETY: CoInitializeEx succeeded for this thread and ShellLink is a COM class
        // provided by Windows. We request the documented IShellLinkW interface and keep
        // all HSTRING arguments alive for each call.
        let shell_link: IShellLinkW =
            unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };

        let executable = hstring(executable);
        let icon = hstring(icon);
        let description = HSTRING::from(APP_COMMENT);
        let working_directory = shortcut_working_directory(shortcut);

        // SAFETY: IShellLinkW methods copy the supplied PCWSTR data during the call.
        // The HSTRING values live until after the calls return, and the paths are
        // absolute paths produced by the installer.
        unsafe {
            shell_link.SetPath(&executable)?;
            shell_link.SetDescription(&description)?;
            shell_link.SetIconLocation(&icon, 0)?;
            if let Some(working_directory) = working_directory {
                shell_link.SetWorkingDirectory(&working_directory)?;
            }
        }

        let persist_file: IPersistFile = shell_link.cast()?;
        let shortcut = hstring(shortcut);
        // SAFETY: The shortcut HSTRING is a valid nul-terminated Windows string for
        // the duration of the call. The parent directory is created by the caller.
        unsafe {
            persist_file.Save(&shortcut, true)?;
        }

        Ok(())
    }

    fn shortcut_working_directory(shortcut: &Path) -> Option<HSTRING> {
        shortcut.parent().map(hstring)
    }

    fn hstring(path: &Path) -> HSTRING {
        HSTRING::from(path.as_os_str().to_string_lossy().as_ref())
    }

    struct ComApartment;

    impl ComApartment {
        fn initialize() -> windows::core::Result<Self> {
            // SAFETY: Initializes COM for the current thread. The returned guard calls
            // CoUninitialize exactly once on drop if initialization succeeds.
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            }
            Ok(Self)
        }
    }

    impl Drop for ComApartment {
        fn drop(&mut self) {
            // SAFETY: This guard is only constructed after a successful CoInitializeEx
            // on the current thread, so matching it with CoUninitialize is required.
            unsafe {
                CoUninitialize();
            }
        }
    }
}

#[cfg(any(target_os = "linux", test))]
fn desktop_exec_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    if path.contains([' ', '\t', '\n', '"', '\\', '`', '$']) {
        format!("\"{}\"", escape_quoted_exec_path(&path))
    } else {
        path.into_owned()
    }
}

#[cfg(any(target_os = "linux", test))]
fn escape_quoted_exec_path(path: &str) -> String {
    let mut escaped = String::with_capacity(path.len());
    for character in path.chars() {
        match character {
            '"' | '\\' | '`' | '$' => {
                escaped.push('\\');
                escaped.push(character);
            }
            _ => escaped.push(character),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "linux")]
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn desktop_entry_matches_app_id_and_icon_name() {
        let executable = Path::new("/opt/dream-ini/bin/dream-ini");
        let entry = desktop_entry(executable);

        assert!(entry.contains("Type=Application\n"));
        assert!(entry.contains("Name=Dream INI\n"));
        assert!(entry.contains("Exec=/opt/dream-ini/bin/dream-ini\n"));
        assert!(entry.contains(&format!("Icon={APP_ID}\n")));
        assert!(entry.contains("Terminal=false\n"));
    }

    #[test]
    fn desktop_entry_quotes_executable_paths_with_spaces() {
        let entry = desktop_entry(Path::new("/opt/Dream INI/bin/dream-ini"));

        assert!(entry.contains("Exec=\"/opt/Dream INI/bin/dream-ini\"\n"));
    }

    #[test]
    fn desktop_entry_escapes_quoted_executable_paths() {
        let entry = desktop_entry(Path::new("/opt/Dream $INI/bin/dream\\ini"));

        assert!(entry.contains("Exec=\"/opt/Dream \\$INI/bin/dream\\\\ini\"\n"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn install_writes_desktop_entry_and_icon() {
        let dir = unique_test_dir("desktop-entry-install");
        let executable = dir.join("bin/dream-ini");

        let paths = install_with_executable(Some(&dir), &executable).unwrap();

        assert_eq!(
            paths.launcher,
            dir.join("applications").join(format!("{APP_ID}.desktop"))
        );
        assert_eq!(
            paths.icon,
            dir.join("icons/hicolor/512x512/apps")
                .join(format!("{APP_ID}.png"))
        );
        assert_eq!(
            std::fs::read_to_string(paths.launcher).unwrap(),
            desktop_entry(&executable)
        );
        assert_eq!(std::fs::read(paths.icon).unwrap(), PNG_ICON_BYTES);

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_install_paths_use_start_menu_and_ico_icon() {
        let app_data = Path::new(r"C:\Users\Tester\AppData\Roaming");

        let paths = windows_install_paths(app_data);

        assert_eq!(
            paths.launcher,
            app_data
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join(format!("{APP_NAME}.lnk"))
        );
        assert_eq!(paths.icon, app_data.join(APP_NAME).join("logo.ico"));
    }

    #[cfg(target_os = "linux")]
    fn unique_test_dir(name: &str) -> PathBuf {
        let temp_dir = env::temp_dir();
        let temp_dir = temp_dir.canonicalize().unwrap_or(temp_dir);
        temp_dir.join(format!(
            "dream-ini-{name}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }
}
