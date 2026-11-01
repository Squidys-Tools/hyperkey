//! Notification-area icon and menu, mirroring the C# TrayIconService: a
//! checkable On/Off item, Open settings, and Quit; the menu opens on
//! right-click and a left double-click opens the settings window.
use hyperkey_core::settings::HyperkeySettings;
use tray_icon::TrayIcon;
use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};

pub const MENU_ID_TOGGLE: &str = "hyperkey.toggle";
pub const MENU_ID_OPEN_SETTINGS: &str = "hyperkey.open-settings";
pub const MENU_ID_QUIT: &str = "hyperkey.quit";

/// The hidden window behind the tray icon must live on the UI thread, so this
/// is created from the daemon's boot function.
pub struct Tray {
    icon: TrayIcon,
    toggle_item: CheckMenuItem,
}

impl Tray {
    pub fn create(settings: &HyperkeySettings) -> Result<Self, String> {
        let enabled = settings.enabled();
        let toggle_item =
            CheckMenuItem::with_id(MENU_ID_TOGGLE, toggle_text(enabled), true, enabled, None);
        let open_item = MenuItem::with_id(MENU_ID_OPEN_SETTINGS, "Open settings", true, None);
        let quit_item = MenuItem::with_id(MENU_ID_QUIT, "Quit", true, None);

        let menu = Menu::new();
        menu.append(&toggle_item)
            .and_then(|()| menu.append(&PredefinedMenuItem::separator()))
            .and_then(|()| menu.append(&open_item))
            .and_then(|()| menu.append(&quit_item))
            .map_err(|error| error.to_string())?;

        let icon = tray_icon::TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip(toggle_text(enabled))
            // Left clicks stay free for the C# double-click-opens-settings
            // gesture; the menu still shows on right click.
            .with_menu_on_left_click(false)
            .with_icon(hyperkey_icon())
            .build()
            .map_err(|error| error.to_string())?;

        Ok(Self { icon, toggle_item })
    }

    pub fn update(&self, settings: &HyperkeySettings) {
        let enabled = settings.enabled();
        self.toggle_item.set_text(toggle_text(enabled));
        self.toggle_item.set_checked(enabled);
        let _ = self.icon.set_tooltip(Some(toggle_text(enabled)));
    }
}

fn toggle_text(enabled: bool) -> &'static str {
    if enabled { "Hyperkey: On" } else { "Hyperkey: Off" }
}

/// Draws the app icon programmatically: the accent square with a white "H",
/// so the tray never depends on an embedded .ico resource.
fn hyperkey_icon() -> tray_icon::Icon {
    const SIZE: usize = 32;
    const ACCENT: [u8; 4] = [0x0B, 0x64, 0xD2, 0xFF];
    const WHITE: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];

    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let edge = x < 2 || y < 2 || x >= SIZE - 2 || y >= SIZE - 2;
            if edge {
                continue;
            }

            let upright = (9..13).contains(&x) || (20..24).contains(&x);
            let crossbar = (14..18).contains(&y) && (9..24).contains(&x);
            let glyph = (9..24).contains(&y) && (upright || crossbar);

            let offset = (y * SIZE + x) * 4;
            rgba[offset..offset + 4].copy_from_slice(if glyph { &WHITE } else { &ACCENT });
        }
    }

    tray_icon::Icon::from_rgba(rgba, SIZE as u32, SIZE as u32)
        .expect("the generated tray icon has valid dimensions and stride")
}
