#[cfg(any(windows, test))]
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::Path;
#[cfg(windows)]
use std::process::{Child, Command, Stdio};
#[cfg(any(windows, test))]
use std::{fs, io, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Profile {
    Chromium,
    Firefox,
    Editor,
    NotepadPlusPlus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Launch {
    Profile(Profile),
    Instance(&'static [&'static str]),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Application {
    pub name: &'static str,
    launch: Launch,
}

impl Application {
    pub fn detect(path: &Path) -> Option<Self> {
        let file = path.file_name()?.to_str()?.to_ascii_lowercase();
        use Launch::{Instance, Profile as SeparateProfile};
        let (name, launch) = match file.as_str() {
            "chrome.exe" => ("Google Chrome", SeparateProfile(Profile::Chromium)),
            "msedge.exe" => ("Microsoft Edge", SeparateProfile(Profile::Chromium)),
            "brave.exe" => ("Brave", SeparateProfile(Profile::Chromium)),
            "chromium.exe" => ("Chromium", SeparateProfile(Profile::Chromium)),
            "vivaldi.exe" => ("Vivaldi", SeparateProfile(Profile::Chromium)),
            "firefox.exe" => ("Mozilla Firefox", SeparateProfile(Profile::Firefox)),
            "code.exe" => ("Visual Studio Code", SeparateProfile(Profile::Editor)),
            "code - insiders.exe" => ("VS Code Insiders", SeparateProfile(Profile::Editor)),
            "vscodium.exe" => ("VSCodium", SeparateProfile(Profile::Editor)),
            "notepad++.exe" => ("Notepad++", SeparateProfile(Profile::NotepadPlusPlus)),
            "winword.exe" => ("Microsoft Word", Instance(&["/w"])),
            "excel.exe" => ("Microsoft Excel", Instance(&["/x"])),
            "vlc.exe" => (
                "VLC media player",
                Instance(&[
                    "--no-one-instance",
                    "--no-one-instance-when-started-from-file",
                ]),
            ),
            _ => return None,
        };
        Some(Self { name, launch })
    }

    #[cfg(windows)]
    pub fn uses_profile(self) -> bool {
        matches!(self.launch, Launch::Profile(_))
    }
}

impl Profile {
    fn arguments(self, profile: &Path) -> Vec<OsString> {
        let directory = profile.as_os_str();
        match self {
            Self::Chromium => {
                let mut argument = OsString::from("--user-data-dir=");
                argument.push(directory);
                vec![
                    argument,
                    "--new-window".into(),
                    "--no-first-run".into(),
                    "about:blank".into(),
                ]
            }
            Self::Firefox => vec!["-no-remote".into(), "-profile".into(), directory.into()],
            Self::Editor => vec![
                "--user-data-dir".into(),
                directory.into(),
                "--extensions-dir".into(),
                profile.join("extensions").into_os_string(),
                "--new-window".into(),
            ],
            Self::NotepadPlusPlus => {
                let mut settings = OsString::from("-settingsDir=");
                settings.push(directory);
                vec!["-multiInst".into(), "-nosession".into(), settings]
            }
        }
    }
}

pub fn extension_is(path: &Path, expected: &str) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

#[cfg(windows)]
pub fn name(path: &Path) -> String {
    Application::detect(path)
        .map(|app| app.name.to_owned())
        .unwrap_or_else(|| {
            path.file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned()
        })
}

#[cfg(any(windows, test))]
fn new_profile(data: &Path, executable: &Path) -> io::Result<PathBuf> {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let family = Sha256::digest(executable.to_string_lossy().to_lowercase().as_bytes());
    let root = data.join("Profiles").join(format!("{family:x}"));
    fs::create_dir_all(&root)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let profile = root.join(format!(
        "instance-{}-{stamp}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&profile)?;
    Ok(profile)
}

#[cfg(windows)]
pub fn executable(path: &Path) -> io::Result<PathBuf> {
    let executable = fs::canonicalize(path)?;
    if !extension_is(&executable, "exe") || !executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Выберите исполняемый файл .exe.",
        ));
    }
    Ok(executable)
}

#[cfg(windows)]
pub fn launch(executable: &Path, data: &Path, arguments: &[OsString]) -> io::Result<Child> {
    let mut command = Command::new(executable);
    if let Some(directory) = executable.parent() {
        command.current_dir(directory);
    }
    let mut profile_dir = None;
    match Application::detect(executable).map(|app| app.launch) {
        Some(Launch::Profile(profile)) => {
            let directory = new_profile(data, executable)?;
            command.args(profile.arguments(&directory));
            profile_dir = Some(directory);
        }
        Some(Launch::Instance(instance)) => {
            command.args(instance);
        }
        None => {
            command.args(arguments);
        }
    }
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(error) => {
            if let Some(directory) = profile_dir {
                let _ = fs::remove_dir(directory);
            }
            Err(error)
        }
    }
}

#[cfg(windows)]
pub fn data_dir() -> io::Result<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .filter(|value| !value.is_empty())
        .map(|value| PathBuf::from(value).join("TwoWindowsInOne"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Папка LocalAppData недоступна."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_launches_never_reuse_a_live_profile() {
        let data = std::env::temp_dir().join(format!("twi-unique-profiles-{}", std::process::id()));
        let exe = Path::new("chrome.exe");
        let first = new_profile(&data, exe).unwrap();
        fs::write(first.join("first-window-data"), "keep").unwrap();
        let second = new_profile(&data, exe).unwrap();
        assert_ne!(first, second);
        assert!(first.join("first-window-data").is_file());
        assert!(!second.join("first-window-data").exists());
        fs::remove_dir_all(data).unwrap();
    }

    #[test]
    fn second_profiles_use_product_specific_arguments() {
        let dir = Path::new("a profile path");
        assert_eq!(
            Application::detect(Path::new("CHROME.EXE")).unwrap().launch,
            Launch::Profile(Profile::Chromium)
        );
        assert_eq!(
            Profile::Chromium.arguments(dir)[0],
            "--user-data-dir=a profile path"
        );

        assert_eq!(
            Profile::Firefox.arguments(dir),
            ["-no-remote", "-profile", "a profile path"]
        );

        let arguments = Profile::Editor.arguments(dir);
        assert_eq!(
            &arguments[0..3],
            ["--user-data-dir", "a profile path", "--extensions-dir"]
        );
        assert_eq!(arguments[3], dir.join("extensions").as_os_str());
        assert_eq!(
            Profile::NotepadPlusPlus.arguments(dir),
            ["-multiInst", "-nosession", "-settingsDir=a profile path"]
        );
        assert_eq!(
            Application::detect(Path::new("WINWORD.EXE"))
                .unwrap()
                .launch,
            Launch::Instance(&["/w"])
        );
        assert_eq!(
            Application::detect(Path::new("EXCEL.EXE")).unwrap().launch,
            Launch::Instance(&["/x"])
        );
        assert_eq!(
            Application::detect(Path::new("vlc.exe")).unwrap().launch,
            Launch::Instance(&[
                "--no-one-instance",
                "--no-one-instance-when-started-from-file"
            ])
        );
        assert!(Application::detect(Path::new("setup.exe")).is_none());
        assert!(extension_is(Path::new("custom.EXE"), "exe"));
        assert!(!extension_is(Path::new("custom.exe.txt"), "exe"));
    }
}
