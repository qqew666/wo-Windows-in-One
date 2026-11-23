use crate::license;
use crate::native::{self, Bounds, DockedWindow};
use crate::profile::{self, Application};
use eframe::egui::{self, Color32, Frame, Margin, RichText, Vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::ptr::null_mut;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use time::{OffsetDateTime, UtcOffset};
use windows_sys::Win32::Foundation::HWND;

const SITE: &str = "https://two-windows-in-one-key-api.sillypetunia8.chatgpt.site/";
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
}

struct Launcher {
    host: HWND,
    data: Option<PathBuf>,
    apps: Vec<(PathBuf, Application)>,
    selection: Option<(PathBuf, Application)>,
    embed: bool,
    key_input: String,
    active_key: String,
    expiry: Option<i64>,
    key_error: Option<String>,
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

        Self {
            host,
            data,
            selection: apps.first().cloned(),
            apps,
            embed: !host.is_null(),
            key_input: active_key.clone(),
            active_key,
            expiry,
            key_error: None,
            status: "Выберите программу и откройте её с отдельным профилем.".into(),
            status_error: false,
            pending: None,
            docked: None,
        }
    }

    fn message(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    fn activate(&mut self) {
        let candidate = self.key_input.trim();
        match license::verify(candidate, unix_now()) {
            Ok(expires) => {
                self.active_key = candidate.to_owned();
                self.expiry = Some(expires);
                self.key_error = None;
                if let Some(dir) = &self.data
                    && let Err(error) = save_key(dir, candidate)
                {
                    self.message(
                        format!(
                            "Ключ активирован на текущий сеанс. Сохранить его не удалось: {error}"
                        ),
                        false,
                    );
                }
            }
            Err(error) => self.key_error = Some(error.message().into()),
        }
    }

    fn launch(&mut self) {
        let Some((path, app)) = self.selection.clone() else {
            return;
        };
        let Some(data) = &self.data else {
            self.message("Папка LocalAppData недоступна.", true);
            return;
        };
        if let Err(error) = license::verify(&self.active_key, unix_now()) {
            self.expiry = None;
            self.key_error = Some(error.message().into());
            return;
        }
        match app.launch(&path, data) {
            Ok(child) if self.embed && !self.host.is_null() => {
                self.pending = Some(Pending {
                    child,
                    started: Instant::now(),
                });
                self.message(
                    format!("Открываю {name} с отдельным профилем…", name = app.name),
                    false,
                );
            }
            Ok(_) => self.message(
                format!(
                    "Открываю {name} с отдельным профилем в своём окне.",
                    name = app.name
                ),
                false,
            ),
            Err(error) => self.message(format!("Запуск не удался: {error}"), true),
        }
    }

    fn browse(&mut self) {
        let chosen = rfd::FileDialog::new()
            .set_title("Выберите файл поддерживаемой программы")
            .add_filter("Программы Windows", &["exe"])
            .pick_file();
        let Some(path) = chosen else { return };
        match Application::detect(&path) {
            Some(app) => {
                self.selection = Some((path, app));
                self.message("Программа выбрана.", false);
            }
            None => self.message(
                "У этой программы пока нет поддерживаемого способа запуска отдельного профиля.",
                true,
            ),
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
                    "Программа передала запуск своему процессу. Проверьте отдельное окно."
                } else {
                    "Второй процесс завершился. Если этот профиль уже открыт, используйте его окно."
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
                    self.pending = None;
                    match DockedWindow::attach(window, self.host, process) {
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
                    self.message("Окно второго процесса пока не найдено. Если программа откроется, она появится отдельно.", false);
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
        ui.label(RichText::new("Поддерживаемые открытые программы").color(MUTED));

        let selected = self
            .selection
            .as_ref()
            .map_or("Выберите программу", |(_, app)| app.name);
        egui::ComboBox::from_id_salt("running-apps")
            .width(ui.available_width())
            .selected_text(selected)
            .show_ui(ui, |ui| {
                if self.apps.is_empty() {
                    ui.label("Открытые программы не найдены.");
                }
                for (path, app) in &self.apps {
                    let active = self
                        .selection
                        .as_ref()
                        .is_some_and(|(selected, _)| selected == path);
                    if ui
                        .selectable_label(active, app.name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        self.selection = Some((path.clone(), *app));
                    }
                }
            });

        ui.horizontal(|ui| {
            if ui.button("Выбрать .exe").clicked() {
                self.browse();
            }
            if ui.button("Обновить список").clicked() {
                self.apps = native::running_apps();
                if self.selection.is_none() {
                    self.selection = self.apps.first().cloned();
                }
            }
        });

        ui.add_enabled_ui(!self.host.is_null(), |ui| {
            ui.checkbox(&mut self.embed, "Встроить окно, если оно совместимо");
        });

        let ready = self.selection.is_some()
            && self.expiry.is_some()
            && self.pending.is_none()
            && self.docked.is_none();
        let launch = egui::Button::new(
            RichText::new("Открыть второй профиль")
                .color(BACKGROUND)
                .strong(),
        )
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
                .hint_text("Вставьте ключ с сайта"),
        );
        let enter = response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        if ui.button("Активировать").clicked() || enter {
            self.activate();
        }

        if let Some(error) = &self.key_error {
            ui.colored_label(ERROR, error);
        } else if let Some(expires) = self.expiry {
            ui.colored_label(ACCENT, format!("Активен до {} мск.", moscow_date(expires)));
        } else {
            ui.colored_label(MUTED, "Получите ключ для запуска второго профиля.");
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
                        if ui.button("Получить ключ на сайте").clicked()
                            && let Err(error) = native::open_external(OsStr::new(SITE))
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
            .show(ctx, |ui| self.controls(ui));

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
        ctx.request_repaint_after(if self.pending.is_some() {
            Duration::from_millis(150)
        } else {
            Duration::from_secs(1)
        });
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
