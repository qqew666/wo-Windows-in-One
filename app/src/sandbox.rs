use crate::profile;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const HELP: &str = "https://learn.microsoft.com/windows/security/application-security/application-isolation/windows-sandbox/windows-sandbox-install";

#[derive(Clone, Copy, Default)]
pub struct Options {
    pub single_file: bool,
    pub network: bool,
}

pub fn local_path(path: &Path) -> io::Result<PathBuf> {
    let canonical = fs::canonicalize(path)?;
    let text = canonical.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Путь к файлу содержит неподдерживаемые символы.",
        )
    })?;
    let path = Path::new(text.strip_prefix(r"\\?\").unwrap_or(text));
    if !path.is_absolute() || path.starts_with(r"\\") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Выберите файл на локальном диске.",
        ));
    }
    Ok(path.to_owned())
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().to_lowercase() == right.to_string_lossy().to_lowercase()
}

fn allow_folder(folder: &Path) -> bool {
    if folder.parent().is_none() {
        return false;
    }
    for variable in [
        "USERPROFILE",
        "WINDIR",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ] {
        let Some(value) = std::env::var_os(variable) else {
            continue;
        };
        let Ok(protected) = local_path(Path::new(&value)) else {
            continue;
        };
        let windows_subfolder = variable == "WINDIR"
            && folder
                .to_string_lossy()
                .to_lowercase()
                .starts_with(&format!(
                    "{}\\",
                    protected
                        .to_string_lossy()
                        .trim_end_matches('\\')
                        .to_lowercase()
                ));
        if same_path(folder, &protected)
            || windows_subfolder
            || (variable == "USERPROFILE"
                && protected.parent().is_some_and(|p| same_path(folder, p)))
        {
            return false;
        }
    }
    true
}

fn xml(value: &Path) -> String {
    value
        .to_string_lossy()
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn configuration(control: &Path, source: &Path, network: bool) -> String {
    let networking = if network { "Enable" } else { "Disable" };
    let control = xml(control);
    let source = xml(source);
    format!(
        r#"<Configuration>
  <Networking>{networking}</Networking>
  <ClipboardRedirection>Disable</ClipboardRedirection>
  <MappedFolders>
    <MappedFolder><HostFolder>{control}</HostFolder><SandboxFolder>C:\TWIControl</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
    <MappedFolder><HostFolder>{source}</HostFolder><SandboxFolder>C:\TWISource</SandboxFolder><ReadOnly>true</ReadOnly></MappedFolder>
  </MappedFolders>
  <LogonCommand><Command>powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File C:\TWIControl\launch.ps1</Command></LogonCommand>
</Configuration>
"#
    )
}

pub fn prepare(path: &Path, data: &Path, options: Options) -> io::Result<PathBuf> {
    let path = local_path(path)?;
    let installer = profile::extension_is(&path, "msi");
    if !path.is_file() || !(installer || profile::extension_is(&path, "exe")) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Выберите файл .exe или .msi.",
        ));
    }
    let name = path.file_name().unwrap();
    let source = path.parent().unwrap();
    if !options.single_file && !allow_folder(source) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Для общей папки диска или Windows включите перенос только выбранного файла.",
        ));
    }

    let sessions = data.join("WindowsSandbox");
    fs::create_dir_all(&sessions)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let session = sessions.join(format!("run-{}-{stamp}", std::process::id()));
    fs::create_dir(&session)?;

    let result = (|| {
        let control = session.join("Control");
        fs::create_dir(&control)?;
        let source = if options.single_file {
            let directory = session.join("App");
            fs::create_dir(&directory)?;
            fs::copy(&path, directory.join(name))?;
            local_path(&directory)?
        } else {
            source.to_owned()
        };

        fs::write(control.join("file.txt"), name.to_string_lossy().as_bytes())?;
        let script: Vec<_> = [0xfeffu16]
            .into_iter()
            .chain(include_str!("guest.ps1").encode_utf16())
            .flat_map(u16::to_le_bytes)
            .collect();
        fs::write(control.join("launch.ps1"), script)?;
        let config = session.join("Two Windows in One.wsb");
        fs::write(
            &config,
            configuration(&local_path(&control)?, &source, options.network),
        )?;
        Ok(config)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(session);
    }
    result
}

pub fn clean_old_sessions(data: &Path) {
    let Ok(entries) = fs::read_dir(data.join("WindowsSandbox")) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let valid_name = name
            .to_str()
            .and_then(|value| value.strip_prefix("run-"))
            .and_then(|value| value.split_once('-'))
            .is_some_and(|(pid, stamp)| {
                pid.parse::<u32>().is_ok() && stamp.parse::<u128>().is_ok()
            });
        if !valid_name {
            continue;
        }
        if entry.file_type().is_ok_and(|kind| kind.is_dir())
            && entry
                .metadata()
                .and_then(|value| value.created())
                .is_ok_and(|created| {
                    created
                        .elapsed()
                        .is_ok_and(|age| age > Duration::from_secs(7 * 86_400))
                })
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_read_only_folders_and_disables_network_by_default() {
        let result = configuration(Path::new(r"C:\Apps & Files"), Path::new(r"C:\Tool"), false);
        assert!(result.contains("Apps &amp; Files"));
        assert_eq!(result.matches("<ReadOnly>true</ReadOnly>").count(), 2);
        assert!(result.contains("<Networking>Disable</Networking>"));
        assert!(result.contains("<ClipboardRedirection>Disable</ClipboardRedirection>"));
        assert!(
            configuration(Path::new("control"), Path::new("source"), true)
                .contains("<Networking>Enable</Networking>")
        );
        assert!(!allow_folder(Path::new(r"C:\")));
    }

    #[test]
    fn prepares_a_single_file_without_sharing_the_original_folder() {
        let base = std::env::temp_dir().join(format!("TWI Sandbox test {}", std::process::id()));
        let input = base.join("Original Folder");
        fs::create_dir_all(&input).unwrap();
        let selected = input.join("Custom application.EXE");
        fs::write(&selected, b"test executable").unwrap();
        fs::write(input.join("untouched.txt"), b"other file").unwrap();
        let config = prepare(
            &selected,
            &base.join("AppData"),
            Options {
                single_file: true,
                network: false,
            },
        )
        .unwrap();
        let session = config.parent().unwrap();
        assert!(session.join("App/Custom application.EXE").is_file());
        assert!(!session.join("App/untouched.txt").exists());
        assert!(
            !fs::read_to_string(config)
                .unwrap()
                .contains("Original Folder")
        );
        fs::remove_dir_all(base).unwrap();
    }
}
