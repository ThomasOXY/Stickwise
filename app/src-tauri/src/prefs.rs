//! The app's settings (the Settings window): the defaults the sheets start
//! from. The frontend owns their shape; here they are kept as JSON and every
//! window hears when they change. Also the Settings window itself and its
//! menu item.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::commands::{CmdResult, err};
use crate::library_cmds::lib;

const PREFS_KEY: &str = "app_prefs";
const WINDOW: &str = "settings";
const MENU_ID: &str = "settings";

/// The saved settings, `null` before the first save.
#[tauri::command]
pub fn prefs_get(app: AppHandle) -> CmdResult<serde_json::Value> {
    Ok(match lib(&app, |l| l.meta(PREFS_KEY))? {
        Some(json) => serde_json::from_str(&json).map_err(err)?,
        None => serde_json::Value::Null,
    })
}

/// Save the settings and tell every window.
#[tauri::command]
pub fn prefs_set(app: AppHandle, prefs: serde_json::Value) -> CmdResult<()> {
    lib(&app, |l| l.set_meta(PREFS_KEY, &prefs.to_string()))?;
    app.emit("prefs-changed", prefs).map_err(err)
}

/// Open the Settings window (or bring it forward) at `section`.
#[tauri::command]
pub fn open_settings(app: AppHandle, section: Option<String>) -> CmdResult<()> {
    open(&app, section.as_deref()).map_err(err)
}

fn open(app: &AppHandle, section: Option<&str>) -> tauri::Result<()> {
    if let Some(w) = app.get_webview_window(WINDOW) {
        if let Some(s) = section {
            w.emit_to(WINDOW, "settings-section", s)?;
        }
        w.unminimize()?;
        w.show()?;
        return w.set_focus();
    }
    let url = match section {
        Some(s) => format!("index.html?window=settings&section={s}"),
        None => "index.html?window=settings".into(),
    };
    WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::App(url.into()))
        .title(i18n::t!("app.settingsWindow"))
        .inner_size(920.0, 640.0)
        .min_inner_size(780.0, 520.0)
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .theme(Some(tauri::Theme::Dark))
        .build()?;
    Ok(())
}

/// The standard menu with Settings… (⌘,) in the app menu.
pub fn menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::default(app)?;
    if let Some(app_menu) = menu.items()?.first().and_then(|i| i.as_submenu().cloned()) {
        let item = MenuItem::with_id(app, MENU_ID, i18n::t!("app.settingsMenu"), true, Some("CmdOrCtrl+,"))?;
        // After "About Stickwise".
        app_menu.insert(&PredefinedMenuItem::separator(app)?, 1)?;
        app_menu.insert(&item, 2)?;
    }
    Ok(menu)
}

pub fn on_menu(app: &AppHandle, id: &str) {
    if id == MENU_ID {
        let _ = open(app, None);
    }
}

/// Use `lang` for the texts from here on: the menu, the Settings window's
/// title and every message the Rust side sends.
#[tauri::command]
pub fn set_locale(app: AppHandle, lang: String) -> CmdResult<()> {
    if lang == i18n::lang() {
        return Ok(());
    }
    i18n::set_lang(&lang);
    app.set_menu(menu(&app).map_err(err)?).map_err(err)?;
    if let Some(w) = app.get_webview_window(WINDOW) {
        w.set_title(&i18n::t!("app.settingsWindow")).map_err(err)?;
    }
    Ok(())
}

/// macOS's preferred languages, most preferred first (`cs-CZ`, `en`, …).
#[tauri::command]
pub fn system_languages() -> Vec<String> {
    let Ok(out) = std::process::Command::new("defaults").args(["read", "-g", "AppleLanguages"]).output() else {
        return Vec::new();
    };
    // A plist array: ( "cs-CZ", "en-CZ" )
    String::from_utf8_lossy(&out.stdout)
        .split(['(', ')', ',', '\n'])
        .map(|l| l.trim().trim_matches('"').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}
