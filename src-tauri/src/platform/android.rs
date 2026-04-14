use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use super::DaemonManager;

static DAEMON_PID: AtomicU32 = AtomicU32::new(0);

pub struct AndroidManager;

pub fn app_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("SIGNET_PATH") {
        return PathBuf::from(custom);
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/data/local/tmp/signet"));
    let exe_dir = exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/data/local/tmp"));

    let candidate = exe_dir.join("../../../files");
    if candidate.exists() {
        return fs::canonicalize(&candidate).unwrap_or(candidate);
    }

    let candidate = exe_dir.join("../../files");
    if candidate.exists() {
        return fs::canonicalize(&candidate).unwrap_or(candidate);
    }

    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/data/data/ai.signet.app/files"))
        .join(".agents")
}

impl AndroidManager {
    fn ensure_dirs(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let data = app_data_dir();
        let daemon_dir = data.join(".daemon");
        let log_dir = daemon_dir.join("logs");
        let bin_dir = data.join("bin");
        let mem_dir = data.join("memory");

        fs::create_dir_all(&daemon_dir)?;
        fs::create_dir_all(&log_dir)?;
        fs::create_dir_all(&bin_dir)?;
        fs::create_dir_all(&mem_dir)?;

        Ok(data)
    }

    fn extract_daemon(&self) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let data = app_data_dir();
        let bin_dir = data.join("bin");
        fs::create_dir_all(&bin_dir)?;

        let daemon_name = format!("signet-daemon-{}", super::target_name());
        let target_path = bin_dir.join(&daemon_name);

        if target_path.exists() {
            return Ok(target_path);
        }

        let fallback = bin_dir.join("signet-daemon");
        if fallback.exists() {
            return Ok(fallback);
        }

        if let Some(bundled) = super::find_bundled_daemon() {
            let src = PathBuf::from(&bundled);
            fs::copy(&src, &target_path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target_path, fs::Permissions::from_mode(0o755))?;
            }

            return Ok(target_path);
        }

        Err("No daemon binary found. The APK must include a signet-daemon binary for aarch64-linux-android.".into())
    }

    fn write_pid(&self, pid: u32) -> Result<(), Box<dyn std::error::Error>> {
        let data = app_data_dir();
        let pid_path = data.join(".daemon/pid");
        let mut f = fs::File::create(&pid_path)?;
        write!(f, "{}", pid)?;
        Ok(())
    }

    fn read_pid_file(&self) -> Option<u32> {
        let data = app_data_dir();
        let pid_path = data.join(".daemon/pid");
        let content = fs::read_to_string(&pid_path).ok()?;
        content.trim().parse().ok()
    }
}

impl DaemonManager for AndroidManager {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if self.is_running() {
            log::info!("Daemon already running, skipping start");
            return Ok(());
        }

        self.ensure_dirs()?;
        let daemon_path = self.extract_daemon()?;
        let data_dir = app_data_dir();

        log::info!("Starting daemon from: {:?}", daemon_path);
        log::info!("SIGNET_PATH: {:?}", data_dir);

        let log_path = data_dir.join(".daemon/logs/daemon.log");
        let log_file = fs::File::create(&log_path)?;

        let child = Command::new(&daemon_path)
            .env("SIGNET_PATH", &data_dir)
            .env("SIGNET_PORT", "3850")
            .env("SIGNET_HOST", "127.0.0.1")
            .env("SIGNET_BIND", "127.0.0.1")
            .env("RUST_LOG", "info")
            .stdout(log_file.try_clone()?)
            .stderr(log_file)
            .spawn()?;

        let pid = child.id();
        DAEMON_PID.store(pid, Ordering::SeqCst);
        self.write_pid(pid)?;

        log::info!("Daemon started with PID: {}", pid);

        let port = crate::commands::daemon_port();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if std::time::Instant::now() > deadline {
                log::warn!("Daemon health check timed out after 10s");
                break;
            }
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                log::info!("Daemon health check passed");
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }

        Ok(())
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pid = self
            .read_pid_file()
            .unwrap_or(DAEMON_PID.load(Ordering::SeqCst));
        if pid == 0 {
            return Ok(());
        }

        log::info!("Stopping daemon PID: {}", pid);

        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }

        for _ in 0..30 {
            if !self.is_running() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if self.is_running() {
            log::warn!("Daemon did not stop gracefully, sending SIGKILL");
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }

        DAEMON_PID.store(0, Ordering::SeqCst);

        let data = app_data_dir();
        let pid_path = data.join(".daemon/pid");
        let _ = fs::remove_file(&pid_path);

        Ok(())
    }

    fn is_running(&self) -> bool {
        let pid = self
            .read_pid_file()
            .unwrap_or(DAEMON_PID.load(Ordering::SeqCst));
        if pid == 0 {
            return false;
        }

        #[cfg(unix)]
        {
            unsafe { libc::kill(pid as i32, 0) == 0 }
        }

        #[cfg(not(unix))]
        {
            false
        }
    }
}
