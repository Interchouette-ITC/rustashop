//! Native application menu (File / Edit / View / Help).

use tauri::menu::{Menu, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Manager, WebviewWindow};

fn main_webview(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window("main").or_else(|| {
        app.webview_windows()
            .into_values()
            .find(|w| w.is_focused().unwrap_or(false))
    })
}

fn reload_webview(app: &AppHandle) {
    let Some(win) = main_webview(app) else {
        return;
    };
    if win.eval("window.location.reload(true)").is_err() {
        let _ = win.eval("window.location.reload()");
    }
}

fn handle_menu_id(app: &AppHandle, id: &str) {
    match id {
        "quit" => app.exit(0),
        "reload" | "forceReload" => reload_webview(app),
        "toggleDevtools" =>
        {
            #[cfg(debug_assertions)]
            if let Some(win) = main_webview(app) {
                if win.is_devtools_open() {
                    win.close_devtools();
                } else {
                    win.open_devtools();
                }
            }
        }
        "resetZoom" => {
            if let Some(win) = main_webview(app) {
                let _ = win.eval("document.body.style.zoom='1'");
            }
        }
        "zoomIn" => {
            if let Some(win) = main_webview(app) {
                let _ = win.eval(
                    "document.body.style.zoom=String((parseFloat(document.body.style.zoom||'1')||1)+0.1)",
                );
            }
        }
        "zoomOut" => {
            if let Some(win) = main_webview(app) {
                let _ = win.eval(
                    "document.body.style.zoom=String(Math.max(0.5,(parseFloat(document.body.style.zoom||'1')||1)-0.1))",
                );
            }
        }
        "goOrders" => {
            if let Some(win) = main_webview(app) {
                let _ = win.eval("window.location.assign('/')");
            }
        }
        "goProducts" => {
            if let Some(win) = main_webview(app) {
                let _ = win.eval("window.location.assign('/products')");
            }
        }
        _ => {}
    }
}

/// Builds the native menu bar.
///
/// # Errors
///
/// Propagates Tauri menu construction errors.
pub fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let quit = MenuItemBuilder::with_id("quit", "Quit")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?;

    let file = SubmenuBuilder::new(app, "File").item(&quit).build()?;

    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, None)?)
        .item(&PredefinedMenuItem::redo(app, None)?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, None)?)
        .item(&PredefinedMenuItem::copy(app, None)?)
        .item(&PredefinedMenuItem::paste(app, None)?)
        .item(&PredefinedMenuItem::select_all(app, None)?)
        .build()?;

    let orders = MenuItemBuilder::with_id("goOrders", "Orders")
        .accelerator("CmdOrCtrl+1")
        .build(app)?;
    let products = MenuItemBuilder::with_id("goProducts", "Products")
        .accelerator("CmdOrCtrl+2")
        .build(app)?;
    let reload = MenuItemBuilder::with_id("reload", "Reload")
        .accelerator("CmdOrCtrl+R")
        .build(app)?;
    let force = MenuItemBuilder::with_id("forceReload", "Force Reload")
        .accelerator("CmdOrCtrl+Shift+F5")
        .build(app)?;
    let zoom_in = MenuItemBuilder::with_id("zoomIn", "Zoom In")
        .accelerator("CmdOrCtrl+Plus")
        .build(app)?;
    let zoom_out = MenuItemBuilder::with_id("zoomOut", "Zoom Out")
        .accelerator("CmdOrCtrl+-")
        .build(app)?;
    let zoom_reset = MenuItemBuilder::with_id("resetZoom", "Actual Size")
        .accelerator("CmdOrCtrl+0")
        .build(app)?;
    let devtools = MenuItemBuilder::with_id("toggleDevtools", "Toggle Developer Tools")
        .accelerator("CmdOrCtrl+Shift+I")
        .build(app)?;

    let view = SubmenuBuilder::new(app, "View")
        .item(&orders)
        .item(&products)
        .separator()
        .item(&reload)
        .item(&force)
        .separator()
        .item(&zoom_reset)
        .item(&zoom_in)
        .item(&zoom_out)
        .separator()
        .item(&devtools)
        .build()?;

    let about = PredefinedMenuItem::about(app, Some("About rustashop admin"), None)?;
    let help = SubmenuBuilder::new(app, "Help").item(&about).build()?;

    Menu::with_items(app, &[&file, &edit, &view, &help])
}

/// Handles native menu events.
pub fn on_menu_event(app: &AppHandle, event: &tauri::menu::MenuEvent) {
    handle_menu_id(app, event.id().as_ref());
}
