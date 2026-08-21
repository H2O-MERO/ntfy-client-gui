use crate::event::{AppEvent, ConnectionFailureReason, EventSender};
use crate::notification;
use crate::ntfy;
use crate::settings::{self, NotificationsMethod, Settings};
use crate::topics::{self, ConnectionKind, SubscribedTopic};
use crate::tray::{Tray, TrayAction};
use crate::updater::{self, UpdateCheckResult};
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct App {
    runtime: tokio::runtime::Handle,
    settings: Settings,
    settings_draft: Settings,
    topics: Vec<TopicEntry>,
    event_tx: EventSender,
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    subscribe_open: bool,
    subscribe_draft: SubscribeDraft,
    settings_open: bool,
    about_open: bool,
    update_dialog_open: bool,
    update_result: Option<UpdateCheckResult>,
    update_in_progress: bool,
    toasts: Vec<Toast>,
    toast_id: u64,
    selected_topic: Option<String>,
    tray: Option<Tray>,
    true_exit: bool,
    visible: bool,
}

struct TopicEntry {
    topic: SubscribedTopic,
    kind: ConnectionKind,
    status: String,
    token: CancellationToken,
}

struct SubscribeDraft {
    topic_id: String,
    server_url: String,
    username: String,
    password: String,
    connection_type: usize,
}

impl Default for SubscribeDraft {
    fn default() -> Self {
        Self {
            topic_id: String::new(),
            server_url: "wss://ntfy.sh".to_string(),
            username: String::new(),
            password: String::new(),
            connection_type: 0,
        }
    }
}

struct Toast {
    id: u64,
    title: String,
    message: String,
    priority: u8,
    created: Instant,
    timeout_secs: f32,
    show_timeout_bar: bool,
    dark: bool,
}

impl App {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: tokio::runtime::Handle,
        start_in_tray: bool,
    ) -> Self {
        install_cjk_font(&cc.egui_ctx);

        let settings = settings::load_settings();
        let settings_draft = settings.clone();
        let loaded_topics = topics::load_topics();
        let (raw_event_tx, event_rx) = mpsc::unbounded_channel();
        let event_tx = EventSender::new(raw_event_tx, cc.egui_ctx.clone());

        let mut app = Self {
            runtime,
            settings,
            settings_draft,
            topics: Vec::new(),
            event_tx,
            event_rx,
            subscribe_open: false,
            subscribe_draft: SubscribeDraft::default(),
            settings_open: false,
            about_open: false,
            update_dialog_open: false,
            update_result: None,
            update_in_progress: false,
            toasts: Vec::new(),
            toast_id: 0,
            selected_topic: None,
            tray: Tray::new(),
            true_exit: false,
            visible: !start_in_tray,
        };

        for topic in loaded_topics {
            app.add_topic_inner(topic, false);
        }

        // 启动后静默检查一次更新。
        app.start_update_check(false);

        app
    }

    // ---------- 公开动作（供托盘调用） ----------

    pub fn show_main_window(&mut self, ctx: &egui::Context) {
        self.visible = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    pub fn request_exit(&mut self, ctx: &egui::Context) {
        self.true_exit = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    pub fn start_update_check(&mut self, interactive: bool) {
        if self.update_in_progress {
            return;
        }
        self.update_in_progress = true;
        let tx = self.event_tx.clone();
        self.runtime.spawn(async move {
            let result = updater::check_for_updates().await;
            let _ = tx.send(AppEvent::UpdateCheckFinished { result, interactive });
        });
    }

    // ---------- 话题管理 ----------

    fn add_topic_inner(&mut self, topic: SubscribedTopic, save: bool) {
        if self.topics.iter().any(|e| e.topic.unique() == topic.unique()) {
            return;
        }

        let kind = topic.connection_kind();
        let token = CancellationToken::new();
        let task_token = token.clone();
        let tx = self.event_tx.clone();
        let reconnect_attempts = self.settings.reconnect_attempts.max(0.0).ceil() as u32;
        let reconnect_attempt_delay = self.settings.reconnect_attempt_delay.max(0.0).ceil() as u64;
        let listen_topic = topic.clone();

        self.runtime.spawn(async move {
            ntfy::listen(
                listen_topic,
                kind,
                tx,
                task_token,
                reconnect_attempts,
                reconnect_attempt_delay,
            )
            .await;
        });

        let status = if topic.has_credentials() {
            "已订阅（认证）".to_string()
        } else {
            "已订阅".to_string()
        };

        self.topics.push(TopicEntry {
            topic,
            kind,
            status,
            token,
        });

        if save {
            topics::save_topics(&self.topics_snapshot());
        }
    }

    fn remove_topic(&mut self, unique: &str) {
        if let Some(pos) = self.topics.iter().position(|e| e.topic.unique() == unique) {
            self.topics[pos].token.cancel();
            self.topics.remove(pos);
            topics::save_topics(&self.topics_snapshot());
        }
    }

    fn topics_snapshot(&self) -> Vec<SubscribedTopic> {
        self.topics.iter().map(|e| e.topic.clone()).collect()
    }

    // ---------- 事件处理 ----------

    fn handle_events(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                AppEvent::Notification {
                    unique,
                    title,
                    message,
                    priority,
                } => {
                    let final_title = if title.trim().is_empty() {
                        unique
                    } else {
                        title
                    };

                    match self.settings.notifications_method {
                        NotificationsMethod::NativeWindows => {
                            let shown = notification::show_native_notification(
                                &final_title,
                                &message,
                            );
                            if !shown {
                                self.show_main_window(ctx);
                                self.push_toast(final_title.clone(), message.clone(), priority);
                            }
                            if self.settings.native_notifications_auto_copy_to_clipboard {
                                copy_to_clipboard(ctx, message.clone());
                            }
                        }
                        NotificationsMethod::CustomTray => {
                            // 自定义弹窗绘制在主窗口内；显示并聚焦主窗口以便用户看到。
                            self.show_main_window(ctx);
                            self.push_toast(final_title, message, priority);
                            if self.settings.custom_tray_notifications_play_default_windows_sound {
                                notification::play_notification_sound();
                            }
                        }
                    }
                }
                AppEvent::ConnectionFailed { unique, reason } => {
                    if let Some(entry) = self.topics.iter_mut().find(|e| e.topic.unique() == unique) {
                        entry.status = format!("连接失败: {}", reason);
                    }
                    let message = match &reason {
                        ConnectionFailureReason::Credentials => format!(
                            "连接话题 {} 失败：{}",
                            unique, reason
                        ),
                        ConnectionFailureReason::MultiAttempt => format!(
                            "连接话题 {} 失败：{}",
                            unique, reason
                        ),
                        ConnectionFailureReason::Other(msg) => format!(
                            "连接话题 {} 失败：{}",
                            unique, msg
                        ),
                    };
                    self.show_main_window(ctx);
                    self.push_toast("连接失败".to_string(), message, 5);
                }
                AppEvent::UpdateCheckFinished { result, interactive } => {
                    self.update_in_progress = false;
                    let should_show = interactive || result.update_available();
                    if should_show {
                        self.show_main_window(ctx);
                    }
                    if let Some(err) = &result.error {
                        if interactive {
                            self.update_dialog_open = false;
                            self.push_toast("检查更新失败".to_string(), err.clone(), 2);
                        }
                    } else if result.update_available() {
                        self.update_result = Some(result);
                        self.update_dialog_open = true;
                    } else if interactive {
                        self.update_dialog_open = false;
                        self.push_toast(
                            "检查更新".to_string(),
                            format!("当前已是最新版本 {}", result.current_version),
                            3,
                        );
                    }
                }
            }
        }
    }

    fn push_toast(&mut self, title: String, message: String, priority: u8) {
        self.toast_id += 1;
        let timeout_secs = if self.settings.timeout <= 0.0 {
            f32::INFINITY
        } else {
            self.settings.timeout as f32
        };
        self.toasts.push(Toast {
            id: self.toast_id,
            title,
            message,
            priority,
            created: Instant::now(),
            timeout_secs,
            show_timeout_bar: self.settings.custom_tray_notifications_show_timeout_bar,
            dark: self.settings.custom_tray_notifications_show_in_dark_mode,
        });

        // 避免同时堆太多弹窗。
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
    }

    // ---------- 订阅对话框逻辑 ----------

    fn submit_subscribe(&mut self) {
        let draft = &self.subscribe_draft;
        let topic_id = draft.topic_id.trim();
        let server_url = draft.server_url.trim();
        let username = draft.username.trim();
        let password = draft.password.trim();

        if topic_id.is_empty() {
            self.push_toast("提示".into(), "必须指定话题名称。".into(), 3);
            return;
        }
        if server_url.is_empty() {
            self.push_toast("提示".into(), "必须指定服务器地址，默认 wss://ntfy.sh".into(), 3);
            return;
        }
        if !username.is_empty() && password.is_empty() {
            self.push_toast("提示".into(), "填写用户名时必须同时填写密码。".into(), 3);
            return;
        }
        if !password.is_empty() && username.is_empty() {
            self.push_toast("提示".into(), "填写密码时必须同时填写用户名。".into(), 3);
            return;
        }

        let use_websocket = draft.connection_type == 0;
        let Some(normalized) = normalize_server_url(server_url, use_websocket) else {
            self.push_toast(
                "无效的服务器地址".into(),
                "支持的协议: http:// https:// ws:// wss://".into(),
                2,
            );
            return;
        };

        let unique = format!("{}@{}", topic_id, normalized);
        if self.topics.iter().any(|e| e.topic.unique() == unique) {
            self.push_toast("提示".into(), format!("话题 {} 已订阅。", unique), 3);
            return;
        }

        let topic = SubscribedTopic {
            topic_id: topic_id.to_string(),
            server_url: normalized,
            username: if username.is_empty() {
                None
            } else {
                Some(username.to_string())
            },
            password: if password.is_empty() {
                None
            } else {
                Some(password.to_string())
            },
        };

        self.add_topic_inner(topic, true);
        self.subscribe_open = false;
    }

    // ---------- 窗口绘制 ----------

    fn draw_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("文件", |ui| {
                    if ui.button("设置").clicked() {
                        self.settings_draft = self.settings.clone();
                        self.settings_open = true;
                        ui.close_menu();
                    }
                    if ui.button("退出").clicked() {
                        self.request_exit(ctx);
                        ui.close_menu();
                    }
                });
                ui.menu_button("帮助", |ui| {
                    if ui.button("检查更新").clicked() {
                        self.start_update_check(true);
                        ui.close_menu();
                    }
                    if ui.button("访问 ntfy.sh").clicked() {
                        let _ = webbrowser::open("https://ntfy.sh/");
                        ui.close_menu();
                    }
                    if ui.button("关于").clicked() {
                        self.about_open = true;
                        ui.close_menu();
                    }
                });
            });
        });
    }

    fn draw_central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("订阅新话题").clicked() {
                    self.subscribe_draft = SubscribeDraft::default();
                    self.subscribe_open = true;
                }
                let can_remove = self.selected_topic.is_some();
                if ui
                    .add_enabled(can_remove, egui::Button::new("删除选中话题"))
                    .clicked()
                {
                    if let Some(unique) = self.selected_topic.take() {
                        self.remove_topic(&unique);
                    }
                }
            });

            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut clicked: Option<String> = None;
                    let mut copy_name: Option<String> = None;
                    let mut copy_address: Option<String> = None;

                    for entry in &self.topics {
                        let unique = entry.topic.unique();
                        let selected = self.selected_topic.as_deref() == Some(unique.as_str());
                        let text = format!(
                            "{}  [{}]  {}",
                            unique,
                            entry.kind.label(),
                            entry.status
                        );
                        let response = ui.selectable_label(selected, text);
                        if response.clicked() {
                            clicked = Some(unique.clone());
                        }
                        response.context_menu(|ui| {
                            if ui.button("复制话题名").clicked() {
                                copy_name = Some(entry.topic.topic_id.clone());
                                ui.close_menu();
                            }
                            if ui.button("复制完整地址").clicked() {
                                copy_address = Some(unique.clone());
                                ui.close_menu();
                            }
                        });
                    }

                    if self.topics.is_empty() {
                        ui.label("暂无订阅话题，点击“订阅新话题”开始。");
                    }

                    if let Some(unique) = clicked {
                        self.selected_topic = Some(unique);
                    }
                    if let Some(name) = copy_name {
                        copy_to_clipboard(ctx, name);
                    }
                    if let Some(address) = copy_address {
                        copy_to_clipboard(ctx, address);
                    }
                });

            ui.separator();
            ui.horizontal(|ui| {
                ui.label("状态:");
                if self.update_in_progress {
                    ui.spinner();
                    ui.label("正在检查更新…");
                } else {
                    ui.label("就绪");
                }
            });
        });
    }

    fn draw_subscribe_window(&mut self, ctx: &egui::Context) {
        let mut open = self.subscribe_open;
        let mut close_requested = false;
        egui::Window::new("订阅新话题")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                egui::Grid::new("subscribe_grid")
                    .num_columns(2)
                    .spacing(egui::vec2(12.0, 8.0))
                    .show(ui, |ui| {
                        ui.label("主题ID");
                        ui.text_edit_singleline(&mut self.subscribe_draft.topic_id);
                        ui.end_row();

                        ui.label("服务器地址");
                        ui.text_edit_singleline(&mut self.subscribe_draft.server_url);
                        ui.end_row();

                        ui.label("用户名");
                        ui.text_edit_singleline(&mut self.subscribe_draft.username);
                        ui.end_row();

                        ui.label("密码");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.subscribe_draft.password)
                                .password(true),
                        );
                        ui.end_row();

                        ui.label("连接方式");
                        let selected_text = if self.subscribe_draft.connection_type == 0 {
                            "Websockets (Recommended)"
                        } else {
                            "Long HTTP JSON (Robust)"
                        };
                        egui::ComboBox::from_id_salt("subscribe_conn_type")
                            .selected_text(selected_text)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.subscribe_draft.connection_type,
                                    0,
                                    "Websockets (Recommended)",
                                );
                                ui.selectable_value(
                                    &mut self.subscribe_draft.connection_type,
                                    1,
                                    "Long HTTP JSON (Robust)",
                                );
                            });
                        ui.end_row();
                    });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("订阅").clicked() {
                        self.submit_subscribe();
                        if !self.subscribe_open {
                            close_requested = true;
                        }
                    }
                    if ui.button("取消").clicked() {
                        close_requested = true;
                    }
                });
            });
        if close_requested {
            open = false;
        }
        self.subscribe_open = open;
    }

    fn draw_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings_open;
        let mut close_requested = false;
        egui::Window::new("设置")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label("通知方式");
                ui.radio_value(
                    &mut self.settings_draft.notifications_method,
                    NotificationsMethod::NativeWindows,
                    "原生 Windows 通知",
                );
                ui.radio_value(
                    &mut self.settings_draft.notifications_method,
                    NotificationsMethod::CustomTray,
                    "自定义托盘通知",
                );
                ui.separator();

                let custom = self.settings_draft.notifications_method
                    == NotificationsMethod::CustomTray;
                ui.add_enabled_ui(custom, |ui| {
                    ui.checkbox(
                        &mut self.settings_draft.custom_tray_notifications_show_timeout_bar,
                        "显示超时进度条",
                    );
                    ui.checkbox(
                        &mut self.settings_draft.custom_tray_notifications_show_in_dark_mode,
                        "深色弹窗",
                    );
                    ui.checkbox(
                        &mut self.settings_draft.custom_tray_notifications_play_default_windows_sound,
                        "播放默认提示音",
                    );
                });
                ui.add_enabled_ui(
                    self.settings_draft.notifications_method
                        == NotificationsMethod::NativeWindows,
                    |ui| {
                        ui.checkbox(
                            &mut self
                                .settings_draft
                                .native_notifications_auto_copy_to_clipboard,
                            "收到通知时自动复制内容到剪贴板",
                        );
                    },
                );
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("通知超时（秒，0=不自动关闭）");
                    ui.add(
                        egui::DragValue::new(&mut self.settings_draft.timeout)
                            .range(0.0..=86400.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("重连次数（0=无限）");
                    ui.add(
                        egui::DragValue::new(&mut self.settings_draft.reconnect_attempts)
                            .range(0.0..=10000.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("重连间隔（秒）");
                    ui.add(
                        egui::DragValue::new(&mut self.settings_draft.reconnect_attempt_delay)
                            .range(0.0..=3600.0),
                    );
                });
                ui.separator();

                ui.checkbox(&mut self.settings_draft.auto_start_enabled, "开机自启动");
                ui.add_enabled_ui(self.settings_draft.auto_start_enabled, |ui| {
                    ui.checkbox(&mut self.settings_draft.auto_start_silent, "自启动时隐藏到托盘");
                });

                ui.horizontal(|ui| {
                    ui.label("语言");
                    let lang = &mut self.settings_draft.language;
                    egui::ComboBox::from_id_salt("language_combo")
                        .selected_text(if lang == "zh-CN" { "中文" } else { "English" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(lang, "zh-CN".to_string(), "中文");
                            ui.selectable_value(lang, "en-US".to_string(), "English");
                        });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("保存").clicked() {
                        self.settings = self.settings_draft.clone();
                        settings::persist_settings(&self.settings);
                        close_requested = true;
                    }
                    if ui.button("取消").clicked() {
                        close_requested = true;
                    }
                });
            });
        if close_requested {
            open = false;
        }
        self.settings_open = open;
    }

    fn draw_about_window(&mut self, ctx: &egui::Context) {
        let mut open = self.about_open;
        let mut close_requested = false;
        egui::Window::new("关于")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.heading("ntfy-client-gui");
                ui.label("ntfy.sh Windows 客户端的 Rust + egui 重写版。");
                ui.label(format!("版本 {}", env!("CARGO_PKG_VERSION")));
                ui.add_space(4.0);
                if ui.button("访问 ntfy.sh").clicked() {
                    let _ = webbrowser::open("https://ntfy.sh/");
                }
                if ui.button("关闭").clicked() {
                    close_requested = true;
                }
            });
        if close_requested {
            open = false;
        }
        self.about_open = open;
    }

    fn draw_update_window(&mut self, ctx: &egui::Context) {
        let mut open = self.update_dialog_open;
        let mut close_requested = false;
        let Some(result) = &self.update_result else {
            self.update_dialog_open = false;
            return;
        };

        egui::Window::new("检查更新")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(err) = &result.error {
                    ui.colored_label(egui::Color32::RED, err);
                    ui.add_space(8.0);
                    if ui.button("关闭").clicked() {
                        close_requested = true;
                    }
                } else if result.update_available() {
                    ui.label(format!(
                        "发现新版本 {}（当前 {}）",
                        result.latest_version.as_deref().unwrap_or("?"),
                        result.current_version
                    ));
                    if let Some(notes) = &result.release_notes {
                        ui.add_space(4.0);
                        ui.label("更新说明：");
                        let mut notes_text = notes.clone();
                        ui.add(
                            egui::TextEdit::multiline(&mut notes_text)
                                .desired_rows(6)
                                .desired_width(360.0),
                        );
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if let Some(url) = &result.release_page_url {
                            if ui.button("打开 Release 页").clicked() {
                                let _ = webbrowser::open(url);
                            }
                        }
                        if let Some(url) = &result.asset_download_url {
                            if ui.button("复制下载链接").clicked() {
                                copy_to_clipboard(ctx, url.clone());
                            }
                        }
                        if ui.button("关闭").clicked() {
                            close_requested = true;
                        }
                    });
                } else {
                    ui.label("当前已是最新版本。");
                    if ui.button("关闭").clicked() {
                        close_requested = true;
                    }
                }
            });
        if close_requested {
            open = false;
        }
        self.update_dialog_open = open;
    }

    fn draw_toasts(&mut self, ctx: &egui::Context) {
        self.toasts
            .retain(|t| t.timeout_secs.is_infinite() || t.created.elapsed().as_secs_f32() < t.timeout_secs);

        let mut remove_id: Option<u64> = None;
        let mut y = 24.0_f32;

        for toast in &self.toasts {
            let id = egui::Id::new(("toast", toast.id));
            let color = notification::priority_color(toast.priority);
            let frame = egui::Frame::none()
                .fill(if toast.dark {
                    egui::Color32::from_rgb(35, 39, 48)
                } else {
                    egui::Color32::from_rgb(250, 250, 250)
                })
                .stroke(egui::Stroke::new(1.0_f32, color))
                .rounding(egui::Rounding::same(8.0));

            egui::Area::new(id)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, y))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    frame.show(ui, |ui| {
                        let (button_bg, button_fg) = if toast.dark {
                            (egui::Color32::from_gray(70), egui::Color32::WHITE)
                        } else {
                            (egui::Color32::from_gray(220), egui::Color32::from_rgb(30, 30, 30))
                        };
                        let visuals = ui.visuals_mut();
                        visuals.override_text_color = Some(button_fg);
                        visuals.widgets.inactive.weak_bg_fill = button_bg;
                        visuals.widgets.hovered.weak_bg_fill = button_bg;
                        visuals.widgets.active.weak_bg_fill = button_bg;
                        visuals.widgets.inactive.bg_fill = button_bg;
                        visuals.widgets.hovered.bg_fill = button_bg;
                        visuals.widgets.active.bg_fill = button_bg;
                        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, button_fg);
                        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, button_fg);
                        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, button_fg);
                        ui.set_width(300.0);
                        ui.horizontal(|ui| {
                            ui.colored_label(color, "●");
                            ui.strong(&toast.title);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    remove_id = Some(toast.id);
                                }
                            });
                        });
                        ui.label(&toast.message);
                        ui.horizontal(|ui| {
                            if ui.small_button("复制").clicked() {
                                copy_to_clipboard(ctx, toast.message.clone());
                            }
                        });
                        if toast.show_timeout_bar && toast.timeout_secs.is_finite() {
                            let elapsed = toast.created.elapsed().as_secs_f32();
                            let remaining = (toast.timeout_secs - elapsed).max(0.0);
                            ui.add(
                                egui::ProgressBar::new(remaining / toast.timeout_secs)
                                    .desired_width(280.0),
                            );
                        }
                    });
                });

            y += 150.0;
        }

        if let Some(id) = remove_id {
            self.toasts.retain(|t| t.id != id);
        }
    }

    fn handle_close_request(&mut self, ctx: &egui::Context) {
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if !close_requested {
            return;
        }

        if self.tray.is_some() && !self.true_exit {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.visible = false;
        } else {
            self.true_exit = true;
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 如果没有托盘，绝不允许窗口“消失”到无法找回。
        if !self.visible && self.tray.is_none() && !self.true_exit {
            self.show_main_window(ctx);
        }

        self.handle_close_request(ctx);
        self.handle_events(ctx);

        let tray_action = self.tray.as_ref().and_then(|tray| tray.poll_events());
        if let Some(action) = tray_action {
            match action {
                TrayAction::Show => self.show_main_window(ctx),
                TrayAction::CheckUpdates => self.start_update_check(true),
                TrayAction::Exit => self.request_exit(ctx),
            }
        }

        self.draw_menu(ctx);
        self.draw_central(ctx);
        self.draw_subscribe_window(ctx);
        self.draw_settings_window(ctx);
        self.draw_about_window(ctx);
        self.draw_update_window(ctx);
        self.draw_toasts(ctx);

        // 有超时弹窗或后台任务时持续刷新界面。
        let has_timed_toasts = self
            .toasts
            .iter()
            .any(|t| t.timeout_secs.is_finite());
        if has_timed_toasts || self.update_in_progress {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

fn install_cjk_font(ctx: &egui::Context) {
    // egui 默认字体不包含 CJK 字形；Windows 下优先加载系统黑体/雅黑。
    let windir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windir.join("Fonts");
    let candidates = [
        fonts_dir.join("simhei.ttf"),
        fonts_dir.join("msyh.ttc"),
        fonts_dir.join("msyh.ttf"),
        fonts_dir.join("simsun.ttc"),
    ];

    for path in candidates {
        if let Ok(bytes) = std::fs::read(&path) {
            // 只保留 CJK 字体，去掉 egui 默认字体，减少内存占用。
            let mut fonts = egui::FontDefinitions {
                font_data: Default::default(),
                families: Default::default(),
            };
            fonts
                .font_data
                .insert("cjk".to_owned(), egui::FontData::from_owned(bytes));
            fonts
                .families
                .insert(egui::FontFamily::Proportional, vec!["cjk".to_owned()]);
            fonts
                .families
                .insert(egui::FontFamily::Monospace, vec!["cjk".to_owned()]);
            ctx.set_fonts(fonts);
            return;
        }
    }
}

fn copy_to_clipboard(ctx: &egui::Context, text: String) {
    ctx.output_mut(|output| output.copied_text = text);
}

fn normalize_server_url(raw: &str, websocket: bool) -> Option<String> {
    let (scheme, rest) = raw.split_once("://")?;
    let rest = rest.trim_end_matches('/');
    let scheme_lower = scheme.to_lowercase();
    let valid = matches!(scheme_lower.as_str(), "http" | "https" | "ws" | "wss");
    if !valid || rest.trim().is_empty() {
        return None;
    }

    let new_scheme = if websocket {
        match scheme_lower.as_str() {
            "http" => "ws",
            "https" => "wss",
            _ => scheme_lower.as_str(),
        }
    } else {
        match scheme_lower.as_str() {
            "ws" => "http",
            "wss" => "https",
            _ => scheme_lower.as_str(),
        }
    };

    Some(format!("{}://{}", new_scheme, rest))
}
