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
        ensure_toast_registration();

        use tauri_winrt_notification::{Duration, Sound, Toast};

        Toast::new("NtfyClientGui")
            .title(title)
            .text1(body)
            .sound(Some(Sound::Default))
            .duration(Duration::Short)
            .add_button("复制内容", "action=copy")
            .show()
            .map(|_| true)
            .unwrap_or(false)
    }
}

/// 在 Windows 上注册 AppUserModelID：
/// 1. 在开始菜单创建带 AUMID 的快捷方式（WinRT toast 必需）；
/// 2. 在注册表写入 AppUserModelId 信息，便于显示名称/图标。
#[cfg(target_os = "windows")]
pub fn ensure_toast_registration() {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, Ordering};
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    static REGISTERED: AtomicBool = AtomicBool::new(false);
    if REGISTERED.swap(true, Ordering::SeqCst) {
        return;
    }

    const AUMID: &str = "NtfyClientGui";
    const DISPLAY_NAME: &str = "Ntfy Client Gui";

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return;
    };

    let start_menu = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs");
    let lnk_path = start_menu.join(format!("{}.lnk", DISPLAY_NAME));

    // 每次都覆盖创建，确保 AUMID/图标/路径是最新的。
    let _ = std::fs::create_dir_all(&start_menu);
    let exe_str = exe.to_string_lossy().to_string();
    let lnk_str = lnk_path.to_string_lossy().to_string();

    if let Ok(mut link) = mslnk::ShellLink::new(exe_str.as_str()) {
        let _ = link.set_icon_location(Some(format!("{},0", exe_str)));
        let _ = link.create_lnk(&lnk_str);
    }

    // 注册表信息（供系统读取显示名称/图标；AUMID 快捷方式属性暂由 mslnk 不支持，先用注册表补充）。
    let reg_path = format!(r"Software\Classes\AppUserModelId\{}", AUMID);
    if let Ok((key, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(&reg_path) {
        let _ = key.set_value("DisplayName", &DISPLAY_NAME.to_string());
        let _ = key.set_value("IconUri", &exe_str);
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
