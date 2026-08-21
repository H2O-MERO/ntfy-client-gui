/// 根据 ntfy 优先级返回一个适合 egui 展示的颜色。
pub fn priority_color(priority: u8) -> egui::Color32 {
    match priority {
        5 => egui::Color32::from_rgb(220, 50, 47),
        4 => egui::Color32::from_rgb(230, 126, 34),
        2 => egui::Color32::from_rgb(52, 152, 219),
        1 => egui::Color32::from_rgb(127, 140, 141),
        _ => egui::Color32::from_rgb(46, 158, 255),
    }
}

/// 发送 Windows 原生 toast。失败时静默返回，由调用方回退到应用内弹窗。
pub fn show_native_notification(title: &str, body: &str) -> bool {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (title, body);
        false
    }

    #[cfg(target_os = "windows")]
    {
        notify_rust::Notification::new()
            .appname("Ntfy Client Gui")
            .summary(title)
            .body(body)
            .action("copy", "复制内容")
            .show()
            .map(|_| true)
            .unwrap_or(false)
    }
}

/// 播放 Windows 默认通知声音（尽力而为）。
pub fn play_notification_sound() {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        // 使用 PowerShell 播放系统提示音，避免额外依赖。
        let _ = Command::new("powershell")
            .args([
                "-NoProfile",
                "-WindowStyle",
                "Hidden",
                "-Command",
                "[console]::beep(1000, 120)",
            ])
            .spawn();
    }

    #[cfg(not(target_os = "windows"))]
    {
        let _ = ();
    }
}
