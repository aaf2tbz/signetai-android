use serde::Deserialize;
use tauri::{AppHandle, Manager};

use crate::daemon;

const DEFAULT_PORT: u16 = 3850;

pub(crate) fn daemon_port() -> u16 {
    std::env::var("SIGNET_PORT")
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT)
}

pub(crate) fn daemon_url() -> String {
    format!("http://localhost:{}", daemon_port())
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)]
pub struct RecentMemory {
    pub content: String,
    pub created_at: String,
    pub who: String,
    pub importance: f64,
}

pub(crate) async fn start_daemon_inner(
    _app: &AppHandle,
) -> Result<(), String> {
    daemon::start().map_err(|e| e.to_string())
}

pub(crate) async fn stop_daemon_inner(
    _app: &AppHandle,
) -> Result<(), String> {
    daemon::stop().map_err(|e| e.to_string())
}

pub(crate) async fn restart_daemon_inner(
    _app: &AppHandle,
) -> Result<(), String> {
    daemon::stop().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(|| {
        std::thread::sleep(std::time::Duration::from_millis(500));
    })
    .await
    .map_err(|e| e.to_string())?;
    daemon::start().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn start_daemon(app: AppHandle) -> Result<(), String> {
    start_daemon_inner(&app).await
}

#[tauri::command]
pub async fn stop_daemon(app: AppHandle) -> Result<(), String> {
    stop_daemon_inner(&app).await
}

#[tauri::command]
pub async fn restart_daemon(app: AppHandle) -> Result<(), String> {
    restart_daemon_inner(&app).await
}

#[tauri::command]
pub async fn get_daemon_pid() -> Result<Option<u32>, String> {
    daemon::read_pid().map_err(|e| e.to_string())
}

pub(crate) fn open_dashboard_inner(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.show().map_err(|e| e.to_string())?;
        win.set_focus().map_err(|e| e.to_string())?;
    } else {
        let _win = tauri::WebviewWindowBuilder::new(
            app,
            "main",
            tauri::WebviewUrl::App("index.html".into()),
        )
        .title("Signet")
        .build()
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn open_dashboard(app: AppHandle) -> Result<(), String> {
    open_dashboard_inner(&app)
}

#[tauri::command]
pub async fn quick_capture(content: String) -> Result<(), String> {
    let client = reqwest::Client::new();
    let base = daemon_url();
    let body = serde_json::json!({
        "content": content,
        "who": "android-capture",
        "importance": 0.7
    });

    let res = client
        .post(format!("{}/api/memory/remember", base))
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    Ok(())
}

pub(crate) async fn ingest_shared_text(text: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let base = daemon_url();
    let body = serde_json::json!({
        "content": text,
        "who": "android-share",
        "importance": 0.7
    });

    let res = client
        .post(format!("{}/api/memory/remember", base))
        .json(&body)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| format!("Failed to ingest shared text: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    Ok(())
}

#[tauri::command]
pub async fn share_text(text: String) -> Result<(), String> {
    ingest_shared_text(&text).await
}

#[tauri::command]
pub async fn search_memories(
    query: String,
    limit: Option<u32>,
) -> Result<String, String> {
    let client = reqwest::Client::new();
    let base = daemon_url();
    let body = serde_json::json!({
        "query": query,
        "limit": limit.unwrap_or(10)
    });

    let res = client
        .post(format!("{}/api/memory/recall", base))
        .json(&body)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("Failed to send: {}", e))?;

    if !res.status().is_success() {
        let status = res.status();
        let text = res.text().await.unwrap_or_default();
        return Err(format!("HTTP {}: {}", status, text));
    }

    let text = res.text().await.map_err(|e| format!("Failed to read body: {}", e))?;
    Ok(text)
}

#[tauri::command]
pub async fn check_for_update(_app: AppHandle) -> Result<Option<String>, String> {
    Ok(None)
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) {
    app.exit(0);
}
