use crate::profile::extension_is;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::process::CommandExt;
use std::path::{self, Path, PathBuf};
use std::process::Command;
use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
use windows_sys::Win32::System::Threading::CREATE_NEW_CONSOLE;
use windows_sys::Win32::System::WindowsProgramming::{GetComputerNameW, GetUserNameW};

pub const SETTINGS: &str = "ms-settings:otherusers";

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

fn local_user(input: &str, computer: &str, current_user: &str) -> io::Result<String> {
    let input = input.trim();
    let (prefix, user) = input.split_once('\\').unwrap_or((".", input));
    if prefix != "." && !prefix.eq_ignore_ascii_case(computer) {
        return Err(invalid(
            "Укажите локальную учётную запись в формате .\\Имя.",
        ));
    }
    if user.trim() != user
        || user.is_empty()
        || user.ends_with('.')
        || user.encode_utf16().count() > 20
        || user
            .chars()
            .any(|ch| ch.is_control() || "/\\[]:;|=,+*?<>@\"".contains(ch))
    {
        return Err(invalid(
            "Введите имя другой локальной учётной записи Windows.",
        ));
    }
    if user.to_lowercase() == current_user.to_lowercase() {
        return Err(invalid(
            "Для второго экземпляра выберите другую учётную запись Windows.",
        ));
    }
    Ok(format!("{computer}\\{user}"))
}

fn windows_name(read: unsafe extern "system" fn(*mut u16, *mut u32) -> i32) -> io::Result<String> {
    let mut buffer = [0u16; 260];
    let mut size = buffer.len() as u32;
    if unsafe { read(buffer.as_mut_ptr(), &mut size) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let name_end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(size as usize);
    Ok(String::from_utf16_lossy(&buffer[..name_end]))
}

pub fn launch(file: &Path, user: &str) -> io::Result<()> {
    let file = path::absolute(file)?;
    if !extension_is(&file, "exe") || !file.is_file() {
        return Err(invalid(
            "Для запуска под другой учётной записью выберите файл .exe.",
        ));
    }
    let text = file
        .to_str()
        .filter(|text| !text.contains('"'))
        .ok_or_else(|| invalid("Не удалось прочитать путь к программе."))?;
    let user = local_user(
        user,
        &windows_name(GetComputerNameW)?,
        &windows_name(GetUserNameW)?,
    )?;

    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    let runas = PathBuf::from(OsString::from_wide(&buffer[..length])).join("runas.exe");
    let mut command = Command::new(runas);
    command
        .args(["/profile", &format!("/user:{user}"), &format!("\"{text}\"")])
        .creation_flags(CREATE_NEW_CONSOLE);
    if let Some(directory) = file.parent() {
        command.current_dir(directory);
    }
    let _ = command.spawn()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_a_different_local_account_and_rejects_invalid_users() {
        assert_eq!(
            local_user(".\\другой", "PC", "Owner").unwrap(),
            "PC\\другой"
        );
        assert_eq!(
            local_user("second account", "PC", "Owner").unwrap(),
            "PC\\second account"
        );
        assert!(local_user("pc\\owner", "PC", "Owner").is_err());
        assert!(local_user("Office\\other", "PC", "Owner").is_err());
        assert!(local_user("another\"user", "PC", "Owner").is_err());
    }
}
