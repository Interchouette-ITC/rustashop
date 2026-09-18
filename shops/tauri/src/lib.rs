//! Thin Tauri 2 shell: native menu + Trunk-built shop Leptos SPA. No privileged invoke commands.

mod menu;

use tauri::Manager;

/// Start the desktop webview and block until the process exits.
///
/// # Panics
///
/// Panics if the Tauri runtime fails to start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let menu = menu::build_menu(app.handle())?;
            app.set_menu(menu)?;
            app.on_menu_event(|app, event| {
                menu::on_menu_event(app, &event);
            });

            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_title(&format!("rustashop · v{}", env!("CARGO_PKG_VERSION")));
                let app_handle = app.handle().clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        app_handle.exit(0);
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running rustashop-shop-tauri");
}
