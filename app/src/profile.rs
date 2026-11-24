#[cfg(any(windows, test))]
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::Path;
#[cfg(windows)]
use std::process::{Child, Command, Stdio};
#[cfg(any(windows, test))]
use std::{fs, io, path::PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Chromium,
    Firefox,
    Editor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Application {
    pub name: &'static str,
    pub kind: Kind,
}

impl Application {
    pub fn detect(path: &Path) -> Option<Self> {
        let file = path.file_name()?.to_str()?.to_ascii_lowercase();
        let (name, kind) = match file.as_str() {
            "chrome.exe" => ("Google Chrome", Kind::Chromium),
            "msedge.exe" => ("Microsoft Edge", Kind::Chromium),
            "brave.exe" => ("Brave", Kind::Chromium),
            "chromium.exe" => ("Chromium", Kind::Chromium),
            "vivaldi.exe" => ("Vivaldi", Kind::Chromium),
            "firefox.exe" => ("Mozilla Firefox", Kind::Firefox),
            "code.exe" => ("Visual Studio Code", Kind::Editor),
            "code - insiders.exe" => ("VS Code Insiders", Kind::Editor),
            "vscodium.exe" => ("VSCodium", Kind::Editor),
            _ => return None,
        };
        Some(Self { name, kind })
    }

    pub fn arguments(self, profile: &Path) -> Vec<OsString> {
        let directory = profile.as_os_str();
        match self.kind {
            Kind::Chromium => {
                let mut argument = OsString::from("--user-data-dir=");
                argument.push(directory);
                vec![
                    argument,
                    "--new-window".into(),
                    "--no-first-run".into(),
                    "about:blank".into(),
                ]
            }
            Kind::Firefox => vec!["-no-remote".into(), "-profile".into(), directory.into()],
            Kind::Editor => vec![
                "--user-data-dir".into(),
                directory.into(),
                "--extensions-dir".into(),
                profile.join("extensions").into_os_string(),
                "--new-window".into(),
            ],
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
pub fn launch(path: &Path, data: &Path) -> io::Result<Child> {
    let canonical = fs::canonicalize(path)?;
    let application = Application::detect(&canonical).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Для этой программы выберите Windows Sandbox.",
        )
    })?;
    if !canonical.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Выбранный файл недоступен.",
        ));
    }

    let profile = new_profile(data, &canonical)?;
    let mut command = Command::new(&canonical);
    if let Some(directory) = canonical.parent() {
        command.current_dir(directory);
    }
    match command
        .args(application.arguments(&profile))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(error) => {
            let _ = fs::remove_dir(&profile);
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
        let chrome = Application::detect(Path::new("CHROME.EXE")).unwrap();
        assert_eq!(chrome.arguments(dir)[0], "--user-data-dir=a profile path");

        let firefox = Application::detect(Path::new("firefox.exe")).unwrap();
        assert_eq!(
            firefox.arguments(dir),
            ["-no-remote", "-profile", "a profile path"]
        );

        let code = Application::detect(Path::new("Code.exe")).unwrap();
        let arguments = code.arguments(dir);
        assert_eq!(
            &arguments[0..3],
            ["--user-data-dir", "a profile path", "--extensions-dir"]
        );
        assert_eq!(arguments[3], dir.join("extensions").as_os_str());
        assert!(Application::detect(Path::new("setup.exe")).is_none());
        assert!(extension_is(Path::new("custom.EXE"), "exe"));
        assert!(!extension_is(Path::new("custom.exe.txt"), "exe"));
    }
}
