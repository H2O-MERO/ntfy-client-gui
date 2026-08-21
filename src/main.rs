#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod event;
mod notification;
mod ntfy;
mod settings;
mod topics;
mod tray;
mod updater;

use app::App;
use eframe::egui;
use std::fs::OpenOptions;

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        let help = "ntfy-client-gui\n\n\
             Usage:\n  \
             -h, --help                    Show this help\n  \
             -t, --start-in-tray           Start hidden tray process\n  \
             --tray                        Start tray-only background process\n  \
             --check-update                Start GUI and check for updates\n  \
             -m, --allow-multiple-instances Allow multiple instances (accepted; not enforced)";
        println!("{}", help);
        rfd::MessageDialog::new()
            .set_title("Help Menu")
            .set_description(help)
            .show();
        return Ok(());
    }

    // 注册 Windows toast AppUserModelID，确保原生通知显示为 “Ntfy Client GUI”。
    #[cfg(target_os = "windows")]
    notification::ensure_toast_registration();

    let tray_mode = args
        .iter()
        .any(|a| a == "--tray" || a == "--start-in-tray" || a == "-t");

    if tray_mode {
        return run_tray_process();
    }

    let check_update = args.iter().any(|a| a == "--check-update");
    run_gui_process(check_update)
}

fn run_gui_process(check_update: bool) -> Result<(), eframe::Error> {
    // 先拉起/确保托盘进程存在。
    spawn_tray_process_if_needed();

    // GUI 单实例锁：已有一个 GUI 时，新进程直接退出。
    let gui_lock_path = settings::data_dir().join("gui.lock");
    let gui_lock = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&gui_lock_path)
    {
        Ok(file) => Some(file),
        Err(_) => {
            eprintln!("GUI already running");
            return Ok(());
        }
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");
    let handle = runtime.handle().clone();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 560.0])
            .with_min_inner_size([520.0, 400.0])
            .with_title("ntfy client GUI")
            .with_resizable(false)
            .with_visible(true),
        ..Default::default()
    };

    let result = eframe::run_native(
        "ntfy-client-gui",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, handle.clone(), false, false, check_update)))),
    );

    drop(gui_lock);
    let _ = std::fs::remove_file(&gui_lock_path);
    result
}

fn run_tray_process() -> Result<(), eframe::Error> {
    // 托盘单实例锁：已有一个托盘进程时，新进程直接退出。
    let tray_lock_path = settings::data_dir().join("tray.lock");
    let tray_lock = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tray_lock_path)
    {
        Ok(file) => Some(file),
        Err(_) => return Ok(()),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1.0, 1.0])
            .with_visible(false),
        ..Default::default()
    };

    let result = eframe::run_native(
        "ntfy-client-gui-tray",
        options,
        Box::new(|cc| Ok(Box::new(TrayApp::new(cc)))),
    );

    drop(tray_lock);
    let _ = std::fs::remove_file(&tray_lock_path);
    result
}

fn spawn_tray_process_if_needed() {
    let tray_lock_path = settings::data_dir().join("tray.lock");
    if tray_lock_path.exists() {
        return;
    }

    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--tray").spawn();
    }
}

struct TrayApp {
    _tray: tray::Tray,
}

impl TrayApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let tray = tray::Tray::new().expect("failed to create tray");
        let ctx = cc.egui_ctx.clone();

        let (show_id, check_id, exit_id) = (
            tray.show_item.id().clone(),
            tray.check_item.id().clone(),
            tray.exit_item.id().clone(),
        );

        tray_icon::menu::MenuEvent::set_event_handler(Some(Box::new(
            move |event: tray_icon::menu::MenuEvent| {
                if event.id == show_id {
                    spawn_gui(false);
                } else if event.id == check_id {
                    spawn_gui(true);
                } else if event.id == exit_id {
                    let _ = std::fs::remove_file(settings::data_dir().join("tray.lock"));
                    std::process::exit(0);
                }
            },
        )));

        tray_icon::TrayIconEvent::set_event_handler(Some(Box::new(
            move |event: tray_icon::TrayIconEvent| {
                if let tray_icon::TrayIconEvent::Click {
                    button,
                    button_state,
                    ..
                } = event
                {
                    if button == tray_icon::MouseButton::Left
                        && button_state == tray_icon::MouseButtonState::Up
                    {
                        spawn_gui(false);
                    }
                }
            },
        )));

        // 保活：确保隐藏窗口的 tray 事件循环持续被唤醒。
        let heartbeat_ctx = ctx.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            heartbeat_ctx.request_repaint();
        });

        Self { _tray: tray }
    }
}

impl eframe::App for TrayApp {
    fn update(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {}
}

fn spawn_gui(check_update: bool) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let mut cmd = std::process::Command::new(exe);
    if check_update {
        cmd.arg("--check-update");
    }
    let _ = cmd.spawn();
}
