use crate::license;
use crate::native::{self, Bounds, DockedWindow};
use crate::{account, online, profile, sandbox};
use eframe::egui::{self, Color32, Frame, Margin, RichText, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::ptr::null_mut;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, UtcOffset};
use windows_sys::Win32::Foundation::HWND;

const BACKGROUND: Color32 = Color32::from_rgb(17, 23, 20);
const SURFACE: Color32 = Color32::from_rgb(27, 38, 31);
const FIELD: Color32 = Color32::from_rgb(36, 49, 40);
const BORDER: Color32 = Color32::from_rgb(62, 83, 68);
const TEXT: Color32 = Color32::from_rgb(239, 247, 239);
const MUTED: Color32 = Color32::from_rgb(175, 196, 179);
const ACCENT: Color32 = Color32::from_rgb(185, 247, 94);
const ERROR: Color32 = Color32::from_rgb(255, 180, 169);

pub fn run() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1100.0, 700.0])
        .with_min_inner_size([900.0, 640.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../site/logo.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Two Windows in One",
        options,
        Box::new(|context| {
            let host = match context.window_handle().ok().map(|handle| handle.as_raw()) {
                Some(RawWindowHandle::Win32(window)) => window.hwnd.get() as HWND,
                _ => null_mut(),
            };
            Ok(Box::new(Launcher::new(&context.egui_ctx, host)))
        }),
    )
}

struct Pending {
    child: Child,
    started: Instant,
    embed: bool,
}

enum OnlineResult {
    Activation(Result<String, String>),
    UpdateCheck(Result<Option<online::Release>, String>),
    Installer(Result<PathBuf, String>),
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum LaunchMode {
    Profile,
    Account,
    Sandbox,
}

impl LaunchMode {
    fn for_file(path: &Path) -> Self {
        if profile::Application::detect(path).is_some() {
            Self::Profile
        } else {
            Self::Sandbox
        }
    }
}

struct Launcher {
    host: HWND,
    data: Option<PathBuf>,
    apps: Vec<PathBuf>,
    selection: Option<PathBuf>,
    sandbox_input: Option<PathBuf>,
    mode: LaunchMode,
    account_name: String,
    sandbox_options: sandbox::Options,
    share_folder: bool,
    sandbox_job: Option<Receiver<io::Result<PathBuf>>>,
    embed: bool,
    key_input: String,
    active_key: String,
    expiry: Option<i64>,
    key_error: Option<String>,
    online_job: Option<Receiver<OnlineResult>>,
    update_release: Option<online::Release>,
    update_installer: Option<PathBuf>,
    status: String,
    status_error: bool,
    pending: Option<Pending>,
    docked: Option<DockedWindow>,
}

impl Launcher {
    fn new(ctx: &egui::Context, host: HWND) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(TEXT);
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = SURFACE;
        visuals.extreme_bg_color = FIELD;
        visuals.selection.bg_fill = Color32::from_rgb(74, 106, 57);
        visuals.widgets.inactive.bg_fill = FIELD;
        visuals.widgets.inactive.weak_bg_fill = FIELD;
        visuals.widgets.hovered.bg_fill = BORDER;
        visuals.widgets.hovered.weak_bg_fill = BORDER;
        ctx.set_visuals(visuals);
        ctx.style_mut(|style| {
            style.spacing.item_spacing = Vec2::new(8.0, 9.0);
            style.spacing.button_padding = Vec2::new(13.0, 8.0);
            style
                .text_styles
                .get_mut(&egui::TextStyle::Body)
                .unwrap()
                .size = 15.0;
            style
                .text_styles
                .get_mut(&egui::TextStyle::Button)
                .unwrap()
                .size = 15.0;
        });

        let data = profile::data_dir().ok();
        let active_key = data
            .as_ref()
            .and_then(|dir| fs::read_to_string(dir.join("license.txt")).ok())
            .map(|key| key.trim().to_owned())
            .filter(|key| key.len() <= 512)
            .unwrap_or_default();
        let expiry = license::verify(&active_key, unix_now()).ok();
        let apps = native::running_apps();
        let sandbox_input = apps.first().and_then(|path| sandbox::local_path(path).ok());
        let mode = apps
            .first()
            .map_or(LaunchMode::Profile, |path| LaunchMode::for_file(path));

        Self {
            host,
            data,
            selection: apps.first().cloned(),
            apps,
            sandbox_input,
            mode,
            account_name: String::new(),
            sandbox_options: sandbox::Options::default(),
            share_folder: false,
            sandbox_job: None,
            embed: false,
            key_input: String::new(),
            active_key,
            expiry,
            key_error: None,
            online_job: None,
            update_release: None,
            update_installer: None,
            status: "Выберите открытое приложение или файл на диске.".into(),
            status_error: false,
            pending: None,
            docked: None,
        }
    }

    fn message(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    fn start_online(&mut self, task: impl FnOnce() -> OnlineResult + Send + 'static) {
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = send.send(task());
        });
        self.online_job = Some(receive);
    }

    fn activate(&mut self) {
        if self.online_job.is_some() {
            return;
        }
        let candidate = self.key_input.trim().to_owned();
        if candidate.starts_with("TWI1.") {
            self.accept_grant(candidate);
        } else if let Some(code) = online::short_code(&candidate) {
            self.key_error = None;
            self.message("Проверяю ключ на сайте…", false);
            self.start_online(move || OnlineResult::Activation(online::activate(&code)));
        } else {
            self.key_error = Some("Введите ключ из 10 символов с сайта.".into());
        }
    }

    fn accept_grant(&mut self, candidate: String) {
        match license::verify(&candidate, unix_now()) {
            Ok(expires) => {
                self.expiry = Some(expires);
                self.key_error = None;
                if let Some(dir) = &self.data
                    && let Err(error) = save_key(dir, &candidate)
                {
                    self.message(format!("Ключ действует до закрытия приложения. Сохранить его не удалось ({error})."), false);
                } else {
                    self.message("Ключ активирован.", false);
                }
                self.active_key = candidate;
            }
            Err(error) => self.key_error = Some(error.message().into()),
        }
    }

    fn poll_online(&mut self) {
        let Some(job) = &self.online_job else { return };
        let result = match job.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.online_job = None;
                self.message("Запрос прерван. Повторите его.", true);
                return;
            }
        };
        self.online_job = None;
        match result {
            OnlineResult::Activation(Ok(grant)) => self.accept_grant(grant),
            OnlineResult::Activation(Err(error)) => {
                self.message("Активация не завершена.", true);
                self.key_error = Some(error);
            }
            OnlineResult::UpdateCheck(Ok(release)) => {
                let message = release.as_ref().map_or_else(
                    || {
                        format!(
                            "Установлена актуальная версия {}.",
                            env!("CARGO_PKG_VERSION")
                        )
                    },
                    |release| format!("Доступно обновление {}.", release.version),
                );
                self.update_release = release;
                self.message(message, false);
            }
            OnlineResult::Installer(Ok(path)) => {
                self.update_installer = Some(path);
                self.message(
                    "Установщик проверен. Нажмите «Установить обновление».",
                    false,
                );
            }
            OnlineResult::UpdateCheck(Err(error)) | OnlineResult::Installer(Err(error)) => {
                self.message(error, true)
            }
        }
    }

    fn update_action(&mut self, ctx: &egui::Context) {
        if self.online_job.is_some() {
            return;
        }
        if let Some(release) = self.update_release.clone() {
            if let Some(path) = self.update_installer.clone() {
                if let Err(error) = online::check_download(&path, &release) {
                    self.update_installer = None;
                    self.message(error, true);
                    return;
                }
                if !self.detach() {
                    return;
                }
                match Command::new(path).arg("/NORESTART").spawn() {
                    Ok(_) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    Err(error) => {
                        self.message(format!("Не удалось открыть установщик ({error})."), true)
                    }
                }
            } else if let Some(data) = self.data.clone() {
                self.message("Скачиваю установщик с GitHub…", false);
                self.start_online(move || {
                    OnlineResult::Installer(online::download(&release, &data))
                });
            } else {
                self.message("Папка LocalAppData недоступна.", true);
            }
        } else {
            self.message("Проверяю обновления на GitHub…", false);
            self.start_online(|| OnlineResult::UpdateCheck(online::check_update()));
        }
    }

    fn launch(&mut self) {
        let Some(path) = self.selection.clone() else {
            return;
        };
        let Some(data) = self.data.clone() else {
            self.message("Папка LocalAppData недоступна.", true);
            return;
        };
        if let Err(error) = license::verify(&self.active_key, unix_now()) {
            self.expiry = None;
            self.key_error = Some(error.message().into());
            return;
        }
        if self.mode == LaunchMode::Account {
            match account::launch(&path, self.account_name.trim()) {
                Ok(()) => self.message(
                    "Введите пароль второй учётной записи в окне Windows Runas. Программа откроется отдельно.",
                    false,
                ),
                Err(error) => self.message(format!("Запуск под другой учётной записью не удался ({error})."), true),
            }
            return;
        }
        if self.mode == LaunchMode::Sandbox {
            let Some(source) = self.sandbox_input.clone() else {
                self.message(
                    "Для запуска в песочнице выберите файл на локальном диске.",
                    true,
                );
                return;
            };
            if !self.sandbox_options.single_file && !self.share_folder {
                return;
            }
            if native::running_apps().iter().any(|candidate| {
                candidate
                    .file_stem()
                    .and_then(OsStr::to_str)
                    .is_some_and(|stem| {
                        matches!(
                            stem.to_ascii_lowercase().as_str(),
                            "windowssandbox"
                                | "windowssandboxclient"
                                | "windowssandboxremotesession"
                        )
                    })
            }) {
                self.message(
                    "Закройте активную песочницу Windows перед запуском новой.",
                    true,
                );
                return;
            }
            let (send, receive) = mpsc::channel();
            let options = self.sandbox_options;
            std::thread::spawn(move || {
                sandbox::clean_old_sessions(&data);
                let _ = send.send(sandbox::prepare(&source, &data, options));
            });
            self.sandbox_job = Some(receive);
            self.message("Подготавливаю файл для Windows Sandbox…", false);
            return;
        }
        match profile::launch(&path, &data) {
            Ok(child) => {
                self.pending = Some(Pending {
                    child,
                    started: Instant::now(),
                    embed: self.embed && !self.host.is_null(),
                });
                self.message(format!("Запускаю {}…", profile::name(&path)), false);
            }
            Err(error) => self.message(
                format!("Запуск не удался ({error}). Попробуйте другую учётную запись или Windows Sandbox."),
                true,
            ),
        }
    }

    fn select_path(&mut self, path: PathBuf) {
        if self.mode != LaunchMode::Account || !profile::extension_is(&path, "exe") {
            self.mode = LaunchMode::for_file(&path);
        }
        self.sandbox_options.single_file = profile::extension_is(&path, "msi");
        self.share_folder = false;
        self.sandbox_input = sandbox::local_path(&path).ok();
        self.selection = Some(path);
        self.message("Программа выбрана.", false);
    }

    fn browse(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Выберите программу или установщик")
            .add_filter("Программы и установщики", &["exe", "msi"])
            .pick_file()
        {
            self.select_path(path);
        }
    }

    fn poll_sandbox(&mut self) {
        let Some(job) = &self.sandbox_job else { return };
        let outcome = match job.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(io::Error::other("Подготовка файла прервана.")),
        };
        self.sandbox_job = None;
        match outcome {
            Ok(config) => match native::open_external(config.as_os_str()) {
                Ok(()) => self.message(
                    "Запуск передан Windows Sandbox. Дождитесь окна отдельной среды.",
                    false,
                ),
                Err(_) => {
                    if let Some(session) = config.parent() {
                        let _ = fs::remove_dir_all(session);
                    }
                    self.message("Windows Sandbox не открылась. Проверьте компонент Windows и аппаратную виртуализацию.", true);
                }
            },
            Err(error) => self.message(format!("Песочница не подготовлена ({error})."), true),
        }
    }

    fn poll_window(&mut self) {
        if self
            .docked
            .as_ref()
            .is_some_and(|window| !window.is_attached())
        {
            self.docked = None;
            self.message("Встроенное окно закрыто или отсоединено программой.", false);
        }
        let Some(pending) = self.pending.as_mut() else {
            return;
        };
        match pending.child.try_wait() {
            Ok(Some(status)) => {
                self.pending = None;
                let hint = if status.success() {
                    "Команда завершилась. Если открылось прежнее окно, попробуйте Windows Sandbox."
                } else {
                    "Второй процесс завершился. Для приложения с ограничением запуска попробуйте Windows Sandbox."
                };
                self.message(hint, !status.success());
            }
            Err(error) => {
                self.pending = None;
                self.message(
                    format!("Не удалось проверить второй процесс: {error}"),
                    true,
                );
            }
            Ok(None) => {
                let process = pending.child.id();
                if let Some(window) = native::process_window(process) {
                    let attachment = pending
                        .embed
                        .then(|| DockedWindow::attach(window, self.host, process));
                    self.pending = None;
                    let Some(attachment) = attachment else {
                        self.message("Новый процесс открыл отдельное окно.", false);
                        return;
                    };
                    match attachment {
                        Ok(docked) => {
                            self.docked = Some(docked);
                            self.message("Второй экземпляр открыт в рабочей области.", false);
                        }
                        Err(error) => self.message(
                            format!(
                                "Программа открылась отдельно. Встраивание недоступно: {error}"
                            ),
                            false,
                        ),
                    }
                } else if pending.started.elapsed() >= Duration::from_secs(18) {
                    self.pending = None;
                    self.message("Окно второго процесса не найдено. Проверьте окна программы или попробуйте Windows Sandbox.", false);
                }
            }
        }
    }

    fn detach(&mut self) -> bool {
        if let Some(window) = &mut self.docked
            && let Err(error) = window.detach()
        {
            self.message(format!("Отсоединить окно не удалось: {error}"), true);
            if self.docked.as_ref().is_some_and(DockedWindow::is_attached) {
                return false;
            }
        }
        self.docked = None;
        true
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.label(RichText::new("Приложение").size(22.0).strong());
        ui.label(RichText::new("Открытые программы").color(MUTED));

        let selected = self.selection.as_ref().map_or_else(
            || "Выберите программу".to_owned(),
            |path| profile::name(path),
        );
        let mut new_selection = None;
        egui::ComboBox::from_id_salt("running-apps")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if self.apps.is_empty() {
                    ui.label("Открытые программы не найдены.");
                }
                for path in &self.apps {
                    if ui
                        .selectable_label(
                            self.selection.as_ref() == Some(path),
                            profile::name(path),
                        )
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        new_selection = Some(path.clone());
                    }
                }
            });
        if let Some(path) = new_selection {
            self.select_path(path);
        }

        ui.horizontal(|ui| {
            if ui.button("Выбрать .exe / .msi").clicked() {
                self.browse();
            }
            if ui.button("Обновить").clicked() {
                self.apps = native::running_apps();
                if self.selection.is_none()
                    && let Some(path) = self.apps.first().cloned()
                {
                    self.select_path(path);
                }
            }
        });

        let native_profile = self
            .selection
            .as_deref()
            .and_then(profile::Application::detect);
        let executable = self
            .selection
            .as_deref()
            .is_some_and(|path| profile::extension_is(path, "exe"));
        ui.add_enabled_ui(native_profile.is_some(), |ui| {
            ui.radio_value(&mut self.mode, LaunchMode::Profile, "Отдельный профиль");
        });
        ui.add_enabled_ui(executable, |ui| {
            ui.radio_value(
                &mut self.mode,
                LaunchMode::Account,
                "Другой пользователь Windows",
            );
        });
        ui.radio_value(&mut self.mode, LaunchMode::Sandbox, "Windows Sandbox");
        match self.mode {
            LaunchMode::Sandbox => {
                if ui
                    .checkbox(
                        &mut self.sandbox_options.single_file,
                        "Перенести только выбранный файл",
                    )
                    .changed()
                {
                    self.share_folder = false;
                }
                if !self.sandbox_options.single_file
                    && let Some(folder) = self.sandbox_input.as_deref().and_then(Path::parent)
                {
                    ui.colored_label(MUTED, folder.display().to_string());
                    ui.checkbox(&mut self.share_folder, "Разрешить чтение этой папки");
                }
                ui.checkbox(&mut self.sandbox_options.network, "Интернет в песочнице");
                ui.hyperlink_to("Как включить Windows Sandbox", sandbox::HELP);
            }
            LaunchMode::Account => {
                ui.label("Имя второй локальной учётной записи");
                ui.add_sized(
                    [ui.available_width(), 34.0],
                    egui::TextEdit::singleline(&mut self.account_name)
                        .char_limit(48)
                        .hint_text(r".\Имя"),
                );
                ui.colored_label(
                    MUTED,
                    "Windows запросит пароль учётной записи. PIN для этого режима не подходит.",
                );
                if ui.button("Учётные записи Windows").clicked()
                    && let Err(error) = native::open_external(OsStr::new(account::SETTINGS))
                {
                    self.message(error.to_string(), true);
                }
            }
            LaunchMode::Profile => {
                ui.add_enabled_ui(!self.host.is_null(), |ui| {
                    ui.checkbox(&mut self.embed, "Встроить совместимое окно");
                });
                ui.colored_label(MUTED, "Каждый запуск использует новую папку профиля.");
            }
        }

        let mode_ready = match self.mode {
            LaunchMode::Profile => native_profile.is_some(),
            LaunchMode::Account => executable && !self.account_name.trim().is_empty(),
            LaunchMode::Sandbox => {
                self.sandbox_input.is_some()
                    && (self.sandbox_options.single_file || self.share_folder)
            }
        };
        let ready = self.selection.is_some()
            && self.expiry.is_some()
            && self.pending.is_none()
            && self.docked.is_none()
            && self.sandbox_job.is_none()
            && mode_ready;
        let label = match self.mode {
            LaunchMode::Profile => "Открыть второй экземпляр",
            LaunchMode::Account => "Открыть от другого пользователя",
            LaunchMode::Sandbox => "Запустить в песочнице",
        };
        let launch = egui::Button::new(RichText::new(label).color(BACKGROUND).strong())
            .fill(ACCENT)
            .min_size(Vec2::new(ui.available_width(), 42.0));
        if ui.add_enabled(ready, launch).clicked() {
            self.launch();
        }

        ui.add_space(12.0);
        ui.separator();
        ui.label(RichText::new("Ключ доступа").size(22.0).strong());

        let response = ui.add_sized(
            [ui.available_width(), 34.0],
            egui::TextEdit::singleline(&mut self.key_input)
                .char_limit(512)
                .hint_text("Ключ из 10 символов"),
        );
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if ui
            .add_enabled(self.online_job.is_none(), egui::Button::new("Активировать"))
            .clicked()
            || enter
        {
            self.activate();
        }

        if let Some(error) = &self.key_error {
            ui.colored_label(ERROR, error);
        } else if let Some(expires) = self.expiry {
            ui.colored_label(ACCENT, format!("Активен до {} мск.", moscow_date(expires)));
        } else {
            ui.colored_label(MUTED, "Получите ключ для запуска программы.");
        }
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|input| input.viewport().close_requested()) {
            self.pending = None;
            if self.detach() {
                return;
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if self.expiry.is_some_and(|end| unix_now() >= end) {
            self.expiry = None;
            self.key_error = Some(license::KeyError::Expired.message().into());
        }
        self.poll_online();
        self.poll_sandbox();
        self.poll_window();

        egui::TopBottomPanel::top("header")
            .frame(
                Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(Margin::symmetric(21, 11)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(37.0), egui::Sense::hover());
                    let painter = ui.painter();
                    painter.rect_filled(rect, 8.0, ACCENT);
                    let back = egui::Rect::from_min_size(
                        rect.min + Vec2::new(7.0, 8.0),
                        Vec2::new(16.0, 13.0),
                    );
                    let front = back.translate(Vec2::new(6.0, 7.0));
                    let line = egui::Stroke::new(2.0_f32, BACKGROUND);
                    painter.rect_stroke(back, 3.0, line, egui::StrokeKind::Inside);
                    painter.rect_filled(front, 3.0, ACCENT);
                    painter.rect_stroke(front, 3.0, line, egui::StrokeKind::Inside);
                    ui.label(RichText::new("Two Windows in One").size(21.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let update_label = self.update_release.as_ref().map_or_else(
                            || "Обновления".to_owned(),
                            |release| {
                                let action = if self.update_installer.is_some() {
                                    "Установить"
                                } else {
                                    "Скачать"
                                };
                                format!("{action} {}", release.version)
                            },
                        );
                        let idle = self.online_job.is_none() && self.sandbox_job.is_none();
                        if ui
                            .add_enabled(idle, egui::Button::new(update_label))
                            .clicked()
                        {
                            self.update_action(ctx);
                        }
                        if ui.button("Релиз в браузере").clicked() {
                            let url = self.update_release.as_ref().map_or_else(
                                || online::RELEASES_PAGE.to_owned(),
                                online::Release::installer_url,
                            );
                            if let Err(error) = native::open_external(OsStr::new(&url)) {
                                self.message(error.to_string(), true);
                            }
                        }
                        if ui.button("Получить ключ на сайте").clicked()
                            && let Err(error) = native::open_external(OsStr::new(online::SITE))
                        {
                            self.message(error.to_string(), true);
                        }
                    });
                });
            });

        egui::TopBottomPanel::bottom("status")
            .frame(
                Frame::new()
                    .fill(BACKGROUND)
                    .inner_margin(Margin::symmetric(21, 9)),
            )
            .show(ctx, |ui| {
                ui.colored_label(if self.status_error { ERROR } else { MUTED }, &self.status);
            });

        egui::SidePanel::left("controls")
            .resizable(false)
            .exact_width(322.0)
            .frame(Frame::new().fill(SURFACE).inner_margin(Margin::same(20)))
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(BACKGROUND).inner_margin(Margin::same(21)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Рабочая область").size(24.0).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.docked.is_some()
                            && ui.button("Отсоединить окно").clicked()
                            && self.detach()
                        {
                            self.message("Окно продолжает работать отдельно.", false);
                        }
                        if self.pending.is_some() && ui.button("Оставить отдельно").clicked()
                        {
                            self.pending = None;
                            self.message("Программа продолжит запуск в отдельном окне.", false);
                        }
                    });
                });
                let (rect, _) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());
                ui.painter().rect_filled(rect, 11.0, SURFACE);
                ui.painter().rect_stroke(
                    rect,
                    11.0,
                    egui::Stroke::new(1.0_f32, BORDER),
                    egui::StrokeKind::Inside,
                );

                if let Some(window) = &mut self.docked {
                    let panel = rect.shrink(5.0);
                    let scale = ctx.pixels_per_point();
                    window.fit(Bounds {
                        x: (panel.min.x * scale).round() as i32,
                        y: (panel.min.y * scale).round() as i32,
                        w: (panel.width() * scale).round().max(1.0) as i32,
                        h: (panel.height() * scale).round().max(1.0) as i32,
                    });
                } else {
                    let text = if self.pending.is_some() {
                        "Ожидаю окно второго процесса…"
                    } else {
                        "Здесь откроется совместимое окно"
                    };
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(18.0),
                        MUTED,
                    );
                }
            });
        ctx.request_repaint_after(
            if self.pending.is_some() || self.sandbox_job.is_some() || self.online_job.is_some() {
                Duration::from_millis(150)
            } else {
                Duration::from_secs(1)
            },
        );
    }
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

fn moscow_date(expires: i64) -> String {
    let stamp = OffsetDateTime::from_unix_timestamp(expires)
        .expect("verified Unix timestamp")
        .to_offset(UtcOffset::from_whole_seconds(license::MOSCOW as i32).unwrap());
    format!(
        "{:02}.{:02}.{:04} {:02}:{:02}",
        stamp.day(),
        stamp.month() as u8,
        stamp.year(),
        stamp.hour(),
        stamp.minute()
    )
}

fn save_key(directory: &Path, key: &str) -> std::io::Result<()> {
    fs::create_dir_all(directory)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let temporary = directory.join(format!("key-{}-{stamp}.tmp", std::process::id()));
    fs::write(&temporary, key)?;
    let result = fs::rename(&temporary, directory.join("license.txt"));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
