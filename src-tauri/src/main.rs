// Almanac desktop app.
//
// On launch it:
//   1. Spawns the Almanac axum server in a background tokio task (the same
//      `almanac_server::run` the CLI uses), listening on a local port.
//   2. Waits for the port to accept connections.
//   3. Shows the native window pointing at the dashboard.
//
// The user never touches a terminal: double-click the .app / .exe / .AppImage
// and the calendar appears.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use almanac_server::Config;
use std::time::Duration;
use tauri::Manager;
use tracing_subscriber::EnvFilter;

/// The port the embedded server listens on. Fixed so the window URL is stable.
const PORT: u16 = 8787;

fn main() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            // Spawn the server. tauri::async_runtime is a tokio multi-thread
            // runtime, so the existing almanac_server::run drops in unchanged.
            tauri::async_runtime::spawn(async move {
                let config = Config {
                    bind: format!("127.0.0.1:{PORT}"),
                    seed_demo: true,
                    ..Config::default()
                };
                if let Err(e) = almanac_server::run(config).await {
                    tracing::error!(error = %e, "almanac server exited");
                }
            });

            // Wait for the port to accept connections, then show the window.
            // (Window starts hidden via tauri.conf.json; shown here to avoid a
            //  flash of "connection refused".)
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let addr = format!("127.0.0.1:{PORT}");
                let mut up = false;
                for _ in 0..80 {
                    if tokio::net::TcpStream::connect(&addr).await.is_ok() {
                        up = true;
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(150)).await;
                }
                if let Some(window) = handle.get_webview_window("main") {
                    if up {
                        let _ = window.show();
                    } else {
                        let _ = window.eval(
                            "document.body.innerHTML='<div style=\"font-family:system-ui;padding:2rem;color:#b91c1c\">Almanac server failed to start. Check the app logs.</div>'"
                        );
                        let _ = window.show();
                    }
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running almanac-app");
}
