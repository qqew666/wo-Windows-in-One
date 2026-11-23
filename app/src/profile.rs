#[cfg(windows)]
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::path::Path;
#[cfg(windows)]
use std::{
    fs, io,
    path::PathBuf,
    process::{Child, Command, Stdio},
};

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

#[cfg(windows)]
pub fn launch(path: &Path, data: &Path) -> io::Result<Child> {
    let canonical = fs::canonicalize(path)?;
    if !canonical.is_file() || !extension_is(&canonical, "exe") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Выберите исполняемый файл .exe.",
        ));
    }

    let mut command = Command::new(&canonical);
    if let Some(directory) = canonical.parent() {
        command.current_dir(directory);
    }
    if let Some(application) = Application::detect(&canonical) {
        // Maintain the same directory for profiles created by earlier releases.
        let identifier = Sha256::digest(canonical.to_string_lossy().to_lowercase().as_bytes());
        let profile = data.join("Profiles").join(format!("{identifier:x}"));
        fs::create_dir_all(&profile)?;
        command
            .args(application.arguments(&profile))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    command.spawn()
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
