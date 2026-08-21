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

fn main() -> Result<(), eframe::Error> {
    env_logger::init();

    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        let help = "ntfy-client-gui\n\n\
             Usage:\n  \
             -h, --help                    Show this help\n  \
             -t, --start-in-tray           Start hidden in the system tray\n  \
             -m, --allow-multiple-instances Allow multiple instances (accepted; single-instance lock is not enforced)";
        println!("{}", help);
        rfd::MessageDialog::new()
            .set_title("Help Menu")
            .set_description(help)
            .show();
        return Ok(());
    }

    let start_in_tray = args.iter().any(|a| a == "-t" || a == "--start-in-tray");
    let _allow_multiple_instances = args
        .iter()
        .any(|a| a == "-m" || a == "--allow-multiple-instances");

    // 注册 Windows toast AppUserModelID，确保原生通知显示为 “Ntfy Client Gui”。
    #[cfg(target_os = "windows")]
    notification::ensure_toast_registration();

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
            .with_visible(!start_in_tray),
        ..Default::default()
    };

    eframe::run_native(
        "ntfy-client-gui",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, handle.clone(), start_in_tray)))),
    )
}
