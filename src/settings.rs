use serde::{Deserialize, Deserializer, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NotificationsMethod {
    NativeWindows,
    CustomTray,
}

impl<'de> Deserialize<'de> for NotificationsMethod {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct NotificationsMethodVisitor;

        impl<'v> serde::de::Visitor<'v> for NotificationsMethodVisitor {
            type Value = NotificationsMethod;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("0, 1, \"NativeWindows\" or \"CustomTray\"")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    0 => Ok(NotificationsMethod::NativeWindows),
                    1 => Ok(NotificationsMethod::CustomTray),
                    _ => Err(E::custom(format!("unknown notifications method: {}", value))),
                }
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == 0.0 {
                    Ok(NotificationsMethod::NativeWindows)
                } else if value == 1.0 {
                    Ok(NotificationsMethod::CustomTray)
                } else {
                    Err(E::custom(format!("unknown notifications method: {}", value)))
                }
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match value {
                    "NativeWindows" | "native_windows" => Ok(NotificationsMethod::NativeWindows),
                    "CustomTray" | "custom_tray" => Ok(NotificationsMethod::CustomTray),
                    _ => Err(E::custom(format!("unknown notifications method: {}", value))),
                }
            }
        }

        deserializer.deserialize_any(NotificationsMethodVisitor)
    }
}

impl Default for NotificationsMethod {
    fn default() -> Self {
        Self::NativeWindows
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
pub struct Settings {
    pub revision: u32,
    /// 通知展示秒数；0 表示不自动关闭（仅自定义弹窗有效）。
    pub timeout: f64,
    /// 重连次数；0 表示无限重试。
    pub reconnect_attempts: f64,
    /// 重连间隔秒数。
    pub reconnect_attempt_delay: f64,
    pub notifications_method: NotificationsMethod,
    pub custom_tray_notifications_show_timeout_bar: bool,
    pub custom_tray_notifications_show_in_dark_mode: bool,
    pub custom_tray_notifications_play_default_windows_sound: bool,
    pub native_notifications_auto_copy_to_clipboard: bool,
    pub auto_start_enabled: bool,
    pub auto_start_silent: bool,
    pub language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            revision: 4,
            timeout: 5.0,
            reconnect_attempts: 10.0,
            reconnect_attempt_delay: 3.0,
            notifications_method: NotificationsMethod::NativeWindows,
            custom_tray_notifications_show_timeout_bar: true,
            custom_tray_notifications_show_in_dark_mode: false,
            custom_tray_notifications_play_default_windows_sound: true,
            native_notifications_auto_copy_to_clipboard: false,
            auto_start_enabled: false,
            auto_start_silent: false,
            language: "zh-CN".to_string(),
        }
    }
}

pub fn data_dir() -> PathBuf {
    // 用户配置文件保存在应用目录（可执行文件旁边），便于绿色携带，也避免写入 AppData。
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let dir = parent.to_path_buf();
            let _ = fs::create_dir_all(&dir);
            return dir;
        }
    }

    PathBuf::from(".")
}

fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

pub fn load_settings() -> Settings {
    let path = settings_path();
    match fs::read_to_string(&path) {
        Ok(text) if !text.trim().is_empty() => {
            let settings: Settings = serde_json::from_str(&text).unwrap_or_default();
            save_settings(&settings);
            settings
        }
        _ => {
            let settings = Settings::default();
            save_settings(&settings);
            settings
        }
    }
}

pub fn save_settings(settings: &Settings) {
    let path = settings_path();
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(path, text);
    }
}

/// 把设置写入磁盘，并在 Windows 上同步“开机自启动”注册表项。
pub fn persist_settings(settings: &Settings) {
    save_settings(settings);
    apply_auto_start(settings);
}

#[cfg(target_os = "windows")]
fn apply_auto_start(settings: &Settings) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::enums::KEY_WRITE;
    use winreg::RegKey;

    const RUN_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run";
    const APP_NAME: &str = "ntfy-client-gui";

    let Ok(hkcu) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE) else {
        return;
    };

    if settings.auto_start_enabled {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let mut command = format!("\"{}\"", exe.display());
        if settings.auto_start_silent {
            command.push_str(" --start-in-tray");
        }
        let _ = hkcu.set_value(APP_NAME, &command);
    } else {
        let _ = hkcu.delete_value(APP_NAME);
    }
}

#[cfg(not(target_os = "windows"))]
fn apply_auto_start(_settings: &Settings) {}
