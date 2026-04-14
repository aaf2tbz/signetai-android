mod commands;
mod daemon;
mod platform;

use tauri::Manager;

#[cfg(not(target_os = "android"))]
use tauri::Listener;

pub fn run() {
    #[cfg(target_os = "android")]
    {
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Info)
                .with_tag("Signet"),
        );
    }

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_shell::init());

    #[cfg(not(target_os = "android"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.show();
                let _ = win.set_focus();
            }
        }));
    }

    builder = builder
        .invoke_handler(tauri::generate_handler![
            commands::start_daemon,
            commands::stop_daemon,
            commands::restart_daemon,
            commands::get_daemon_pid,
            commands::open_dashboard,
            commands::quick_capture,
            commands::search_memories,
            commands::share_text,
            commands::quit_app,
            commands::check_for_update,
        ])
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                }
            }
        });

    builder = builder.setup(|app| {
        #[cfg(target_os = "android")]
        {
            use tauri::Listener;
            app.listen("share-text", |event: tauri::Event| {
                let text = event.payload();
                if !text.is_empty() {
                    let text = text.to_string();
                    tauri::async_runtime::spawn(async move {
                        let _ = commands::ingest_shared_text(&text).await;
                    });
                }
            });
        }

        let port = commands::daemon_port();
        let daemon_up =
            std::net::TcpStream::connect(("127.0.0.1", port)).is_ok();
        if !daemon_up {
            let _ = daemon::start();
        }

        Ok(())
    });

    builder.run(tauri::generate_context!())
        .expect("error while running signet");
}
