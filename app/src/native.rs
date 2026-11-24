use crate::profile;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::{fs, io};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, HANDLE, HWND, LPARAM, LocalFree, SetLastError,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::HiDpi::{AreDpiAwarenessContextsEqual, GetWindowDpiAwarenessContext};
use windows_sys::Win32::UI::Shell::{CommandLineToArgvW, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetParent, GetWindow, GetWindowLongPtrW,
    GetWindowPlacement, GetWindowTextLengthW, GetWindowThreadProcessId, IsHungAppWindow, IsWindow,
    IsWindowVisible, SET_WINDOW_POS_FLAGS, SW_RESTORE, SW_SHOWNORMAL, SWP_ASYNCWINDOWPOS,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SetParent,
    SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow, WINDOWPLACEMENT, WS_CAPTION,
    WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

fn pid(window: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(window, &mut pid) };
    pid
}

fn top_level_windows() -> Vec<HWND> {
    unsafe extern "system" fn collect(window: HWND, ctx: LPARAM) -> i32 {
        // EnumWindows calls this synchronously with a valid Vec pointer.
        let windows = unsafe { &mut *(ctx as *mut Vec<HWND>) };
        windows.push(window);
        1
    }

    let mut windows = Vec::new();
    unsafe { EnumWindows(Some(collect), (&mut windows as *mut Vec<HWND>) as LPARAM) };
    windows
}

fn is_application_window(window: HWND) -> bool {
    unsafe {
        IsWindowVisible(window) != 0
            && GetWindow(window, GW_OWNER).is_null()
            && GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW == 0
            && GetWindowTextLengthW(window) > 0
    }
}

fn process_path(process: u32) -> Option<PathBuf> {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process) };
    if handle.is_null() {
        return None;
    }
    let mut buffer = vec![0; 32_768];
    let mut length = buffer.len() as u32;
    let found = unsafe {
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        );
        CloseHandle(handle);
        result != 0
    };
    found.then(|| PathBuf::from(OsString::from_wide(&buffer[..length as usize])))
}

pub fn running_apps() -> Vec<PathBuf> {
    let mut pids = HashSet::new();
    let mut paths = HashSet::new();
    let mut apps = Vec::new();

    for window in top_level_windows() {
        let process = pid(window);
        if !is_application_window(window)
            || process == 0
            || process == std::process::id()
            || !pids.insert(process)
        {
            continue;
        }
        let Some(path) = process_path(process) else {
            continue;
        };
        if profile::extension_is(&path, "exe")
            && paths.insert(path.to_string_lossy().to_lowercase())
        {
            apps.push(path);
        }
    }
    apps.sort_by_cached_key(|path| (profile::name(path).to_lowercase(), path.clone()));
    apps
}

fn runs_executable(process: u32, executable: &Path) -> bool {
    process_path(process)
        .filter(|path| {
            path.file_name()
                .zip(executable.file_name())
                .is_some_and(|(left, right)| left.eq_ignore_ascii_case(right))
        })
        .and_then(|path| fs::canonicalize(path).ok())
        .is_some_and(|path| {
            path.as_os_str()
                .eq_ignore_ascii_case(executable.as_os_str())
        })
}

pub struct LaunchWindows {
    executable: PathBuf,
    existing: HashSet<(HWND, u32)>,
    original: Vec<(HWND, u32)>,
}

pub struct NewWindow {
    pub handle: HWND,
    pub child_process: Option<u32>,
}

impl LaunchWindows {
    pub fn capture(executable: &Path) -> Self {
        // Include hidden windows so a pre-existing window becoming visible cannot count as new.
        let existing: HashSet<_> = top_level_windows()
            .into_iter()
            .map(|window| (window, pid(window)))
            .collect();
        let original = existing
            .iter()
            .copied()
            .filter(|&(window, process)| {
                is_application_window(window) && runs_executable(process, executable)
            })
            .collect();
        Self {
            executable: executable.to_path_buf(),
            existing,
            original,
        }
    }

    pub fn original_closed(&self) -> bool {
        self.original
            .iter()
            .any(|&(window, process)| pid(window) != process)
    }

    pub fn find(&self, child_process: Option<u32>) -> Option<NewWindow> {
        let mut other_window = None;
        for window in top_level_windows() {
            let process = pid(window);
            if process == 0
                || self.existing.contains(&(window, process))
                || !is_application_window(window)
                || unsafe { IsHungAppWindow(window) != 0 }
            {
                continue;
            }
            if Some(process) == child_process {
                return Some(NewWindow {
                    handle: window,
                    child_process,
                });
            }
            if other_window.is_none() && runs_executable(process, &self.executable) {
                other_window = Some(NewWindow {
                    handle: window,
                    child_process: None,
                });
            }
        }
        other_window
    }
}

pub fn launch_arguments(text: &str) -> io::Result<Vec<OsString>> {
    if text.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Параметры содержат недопустимый символ.",
        ));
    }
    // The first token is parsed as an executable name, so supply a placeholder.
    let command: Vec<_> = "app.exe "
        .encode_utf16()
        .chain(text.encode_utf16())
        .chain([0])
        .collect();
    let mut count = 0;
    let args = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
    if args.is_null() {
        return Err(io::Error::last_os_error());
    }
    let mut result = Vec::new();
    unsafe {
        for index in 1..count {
            let value = *args.add(index as usize);
            let mut length = 0;
            while *value.add(length) != 0 {
                length += 1;
            }
            result.push(OsString::from_wide(std::slice::from_raw_parts(
                value, length,
            )));
        }
        LocalFree(args.cast());
    }
    Ok(result)
}

pub fn open_external(target: &OsStr) -> io::Result<()> {
    shell_execute(target, "open")
}

fn shell_execute(target: &OsStr, operation: &str) -> io::Result<()> {
    let path = Path::new(target);
    let directory: Option<Vec<u16>> = path
        .is_absolute()
        .then(|| path.parent())
        .flatten()
        .map(|folder| folder.as_os_str().encode_wide().chain([0]).collect());
    let target: Vec<_> = target.encode_wide().chain([0]).collect();
    let operation: Vec<_> = operation.encode_utf16().chain([0]).collect();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            directory.as_ref().map_or(null(), |folder| folder.as_ptr()),
            SW_SHOWNORMAL,
        )
    };
    if result as isize <= 32 {
        Err(io::Error::other("Windows не смогла выполнить команду."))
    } else {
        Ok(())
    }
}

// Setup checks this mutex before replacing the executable. Each launcher keeps a handle.
pub struct InstallerLock(HANDLE);

impl Drop for InstallerLock {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn installer_lock() -> Option<InstallerLock> {
    let name: Vec<_> = "Local\\TwoWindowsInOne-3C394A86"
        .encode_utf16()
        .chain([0])
        .collect();
    let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
    if handle.is_null() {
        return None;
    }
    Some(InstallerLock(handle))
}

fn set_long(window: HWND, index: i32, value: isize) -> io::Result<()> {
    unsafe {
        SetLastError(0);
        if SetWindowLongPtrW(window, index, value) == 0 && GetLastError() != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn set_parent(window: HWND, parent: HWND) -> io::Result<()> {
    unsafe {
        SetLastError(0);
        if SetParent(window, parent).is_null() && GetLastError() != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn frame_changed(window: HWND) {
    let flags = SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE;
    unsafe { SetWindowPos(window, null_mut(), 0, 0, 0, 0, flags) };
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

pub struct DockedWindow {
    window: HWND,
    host: HWND,
    process: u32,
    previous_style: isize,
    previous_extended: isize,
    placement: WINDOWPLACEMENT,
    bounds: Option<Bounds>,
}

impl DockedWindow {
    pub fn attach(window: HWND, host: HWND, process: u32) -> io::Result<Self> {
        if window == host
            || pid(window) != process
            || unsafe {
                IsWindow(host) == 0
                    || IsWindow(window) == 0
                    || !GetParent(window).is_null()
                    || IsHungAppWindow(window) != 0
            }
        {
            return Err(io::Error::other(
                "Не удалось подтвердить окно второго процесса.",
            ));
        }

        // Changing the parent across incompatible DPI modes can reset the other application's DPI.
        if unsafe {
            AreDpiAwarenessContextsEqual(
                GetWindowDpiAwarenessContext(window),
                GetWindowDpiAwarenessContext(host),
            ) == 0
        } {
            return Err(io::Error::other(
                "У программы и лаунчера разные режимы масштабирования.",
            ));
        }

        let host_style = unsafe { GetWindowLongPtrW(host, GWL_STYLE) };
        set_long(
            host,
            GWL_STYLE,
            (host_style as u32 | WS_CLIPCHILDREN) as isize,
        )?;

        let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
        let extended = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) };
        let mut placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        if unsafe { GetWindowPlacement(window, &mut placement) } == 0 {
            return Err(io::Error::last_os_error());
        }

        let chrome =
            WS_POPUP | WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX;
        let child_style = (style as u32 & !chrome) | WS_CHILD | WS_CLIPSIBLINGS;
        let child_extended = extended as u32 & !(WS_EX_APPWINDOW | WS_EX_TOPMOST);

        unsafe { ShowWindow(window, SW_RESTORE) };
        let change = set_long(window, GWL_STYLE, child_style as isize)
            .and_then(|_| set_long(window, GWL_EXSTYLE, child_extended as isize))
            .and_then(|_| set_parent(window, host));
        if let Err(error) = change {
            let _ = set_long(window, GWL_STYLE, style);
            let _ = set_long(window, GWL_EXSTYLE, extended);
            unsafe { SetWindowPlacement(window, &placement) };
            frame_changed(window);
            return Err(error);
        }

        frame_changed(window);
        Ok(Self {
            window,
            host,
            process,
            previous_style: style,
            previous_extended: extended,
            placement,
            bounds: None,
        })
    }

    pub fn is_attached(&self) -> bool {
        unsafe {
            IsWindow(self.window) != 0
                && pid(self.window) == self.process
                && GetParent(self.window) == self.host
        }
    }

    pub fn fit(&mut self, bounds: Bounds) {
        if self.bounds == Some(bounds) || !self.is_attached() {
            return;
        }
        let flags: SET_WINDOW_POS_FLAGS = SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS;
        let result = unsafe {
            SetWindowPos(
                self.window,
                null_mut(),
                bounds.x,
                bounds.y,
                bounds.w,
                bounds.h,
                flags,
            )
        };
        if result != 0 {
            self.bounds = Some(bounds);
        }
    }

    pub fn detach(&mut self) -> io::Result<()> {
        if !self.is_attached() {
            return Ok(());
        }
        set_parent(self.window, null_mut())?;

        // Restore both styles even if the first call fails.
        let style = set_long(self.window, GWL_STYLE, self.previous_style);
        let extended = set_long(self.window, GWL_EXSTYLE, self.previous_extended);
        unsafe { SetWindowPlacement(self.window, &self.placement) };
        frame_changed(self.window);
        self.bounds = None;
        style.and(extended)
    }
}

impl Drop for DockedWindow {
    fn drop(&mut self) {
        let _ = self.detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, TranslateMessage,
        WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    fn create_test_window(title: &str) -> HWND {
        let name: Vec<_> = title.encode_utf16().chain([0]).collect();
        let class: Vec<_> = "STATIC".encode_utf16().chain([0]).collect();
        unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                32,
                46,
                360,
                240,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null()),
                null(),
            )
        }
    }

    #[test]
    #[ignore = "native window fixture launched by the embedding test"]
    fn window_fixture() {
        assert_eq!(std::env::var("TWI_WINDOW_FIXTURE").as_deref(), Ok("1"));
        assert!(!create_test_window("Two Windows in One test child").is_null());
        let mut message = MSG::default();
        unsafe {
            while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    struct Fixture(Child);

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn distinguishes_new_windows_and_only_embeds_the_new_process() {
        let executable = profile::executable(&std::env::current_exe().unwrap()).unwrap();
        let host = create_test_window("Two Windows in One test host");
        assert!(!host.is_null());
        let search = LaunchWindows::capture(&executable);
        assert!(search.find(None).is_none());

        let forwarded = create_test_window("Second window in the existing process");
        assert!(!forwarded.is_null());
        let matched = search.find(None).unwrap();
        assert_eq!(matched.handle, forwarded);
        assert!(matched.child_process.is_none());
        assert!(!search.original_closed());
        unsafe { DestroyWindow(forwarded) };

        let fixture = Fixture(
            Command::new(&executable)
                .args(["--exact", "native::tests::window_fixture", "--ignored"])
                .env("TWI_WINDOW_FIXTURE", "1")
                .stdout(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let start = Instant::now();
        let window = loop {
            if let Some(window) = search.find(Some(fixture.0.id())) {
                assert_eq!(window.child_process, Some(fixture.0.id()));
                break window.handle;
            }
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "child window did not open"
            );
            sleep(Duration::from_millis(50));
        };
        let style = unsafe { GetWindowLongPtrW(window, GWL_STYLE) };
        let mut docked = DockedWindow::attach(window, host, fixture.0.id()).unwrap();
        docked.fit(Bounds {
            x: 8,
            y: 8,
            w: 280,
            h: 180,
        });
        assert!(docked.is_attached());
        docked.detach().unwrap();
        assert!(unsafe { GetParent(window) }.is_null());
        assert_eq!(unsafe { GetWindowLongPtrW(window, GWL_STYLE) }, style);
        assert!(unsafe { IsWindow(window) } != 0);
        assert!(!search.original_closed());
        unsafe { DestroyWindow(host) };
        assert!(search.original_closed());
    }

    #[test]
    fn direct_launch_passes_arguments_without_a_shell() {
        let arguments = launch_arguments(r#"--flag "two words" "&" """#).unwrap();
        assert_eq!(arguments, ["--flag", "two words", "&", ""]);
        assert!(launch_arguments("").unwrap().is_empty());
        let executable = profile::executable(&std::env::current_exe().unwrap()).unwrap();
        let mut child =
            profile::launch(&executable, Path::new("unused"), &["--list".into()]).unwrap();
        assert!(child.wait().unwrap().success());
    }
}
