use tray_icon::menu::{Menu, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

pub struct Tray {
    _tray: TrayIcon,
    pub show_item: MenuItem,
    pub check_item: MenuItem,
    pub exit_item: MenuItem,
}

impl Tray {
    pub fn new() -> Option<Self> {
        let show_item = MenuItem::new("显示主窗口", true, None);
        let check_item = MenuItem::new("检查更新", true, None);
        let exit_item = MenuItem::new("退出", true, None);

        let menu = Menu::new();
        menu.append(&show_item).ok()?;
        menu.append(&check_item).ok()?;
        menu.append(&exit_item).ok()?;

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip("ntfy client GUI")
            .with_icon(default_icon()?)
            .build()
            .ok()?;

        Some(Self {
            _tray: tray,
            show_item,
            check_item,
            exit_item,
        })
    }
}

fn default_icon() -> Option<Icon> {
    let size: u32 = 32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let in_circle = dx * dx + dy * dy <= 15 * 15;
            let (r, g, b): (u8, u8, u8) = if in_circle {
                // 简单的蓝色圆形，中间用白色画一个近似的 “n”。
                let in_letter = (x >= 10 && x <= 14 && y >= 12 && y <= 22)
                    || (x >= 18 && x <= 22 && y >= 12 && y <= 22)
                    || (y >= 20 && y <= 22 && x >= 12 && x <= 20);
                if in_letter {
                    (255, 255, 255)
                } else {
                    (46, 158, 255)
                }
            } else {
                (0, 0, 0)
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Icon::from_rgba(rgba, size, size).ok()
}
