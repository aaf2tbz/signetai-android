use std::path::PathBuf;

#[allow(dead_code)]
pub trait DaemonManager {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn stop(&self) -> Result<(), Box<dyn std::error::Error>>;
    fn is_running(&self) -> bool;
}

#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

pub fn data_dir() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        android::app_data_dir()
    }

    #[cfg(not(target_os = "android"))]
    {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".agents")
    }
}

fn target_name() -> &'static str {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    const TARGET: &str = "x86_64-unknown-linux-gnu";
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    const TARGET: &str = "aarch64-unknown-linux-gnu";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    const TARGET: &str = "x86_64-apple-darwin";
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    const TARGET: &str = "aarch64-apple-darwin";
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    const TARGET: &str = "x86_64-pc-windows-msvc";
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    const TARGET: &str = "aarch64-pc-windows-msvc";
    #[cfg(target_os = "android")]
    const TARGET: &str = "aarch64-linux-android";
    #[cfg(not(any(
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
        target_os = "android"
    )))]
    const TARGET: &str = "unknown";

    TARGET
}

fn ext() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        return ".exe";
    }

    #[cfg(not(target_os = "windows"))]
    {
        return "";
    }
}

fn preferred_name() -> String {
    format!("signet-daemon-{}{}", target_name(), ext())
}

fn fallback_name() -> String {
    format!("signet-daemon{}", ext())
}

fn find_by_name(dir: &PathBuf, name: &str) -> Option<String> {
    let path = dir.join(name);
    if path.is_file() {
        return Some(path.to_string_lossy().to_string());
    }
    None
}

pub fn find_bundled_daemon() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let root = exe.parent()?;

    #[cfg(target_os = "android")]
    let dirs = {
        let data = app_data_dir();
        vec![data.join("bin"), root.to_path_buf()]
    };

    #[cfg(all(not(target_os = "android"), target_os = "macos"))]
    let dirs = vec![root.to_path_buf(), root.join("../Resources")];

    #[cfg(all(not(target_os = "android"), not(target_os = "macos")))]
    let dirs = vec![root.to_path_buf()];

    for dir in dirs {
        if let Some(path) = find_by_name(&dir, &preferred_name()) {
            return Some(path);
        }
        if let Some(path) = find_by_name(&dir, &fallback_name()) {
            return Some(path);
        }
    }

    None
}

#[cfg(target_os = "android")]
fn app_data_dir() -> PathBuf {
    android::app_data_dir()
}

pub fn create_manager() -> Box<dyn DaemonManager> {
    #[cfg(target_os = "android")]
    {
        Box::new(android::AndroidManager)
    }

    #[cfg(target_os = "linux")]
    {
        Box::new(linux::LinuxManager)
    }

    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacosManager)
    }

    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WindowsManager)
    }
}
