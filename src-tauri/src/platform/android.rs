use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use super::DaemonManager;

static DAEMON_PID: AtomicU32 = AtomicU32::new(0);
static LLAMA_EMBED_PID: AtomicU32 = AtomicU32::new(0);
static LLAMA_LLM_PID: AtomicU32 = AtomicU32::new(0);

pub struct AndroidManager;

pub fn app_data_dir() -> PathBuf {
    if let Ok(custom) = std::env::var("SIGNET_PATH") {
        return PathBuf::from(custom);
    }

    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("/data/local/tmp/signet"));
    let exe_dir = exe
        .parent()
        .unwrap_or_else(|| std::path::Path::new("/data/local/tmp"));

    for suffix in &["../../../files", "../../files", "../files"] {
        let candidate = exe_dir.join(suffix).join(".agents");
        if candidate.exists() {
            return fs::canonicalize(&candidate).unwrap_or(candidate);
        }
    }

    PathBuf::from("/data/data/ai.signet.app/files/.agents")
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

        let daemon_path = bin_dir.join("signet-daemon");
        if daemon_path.is_file() {
            return Ok(daemon_path);
        }

        let daemon_named = bin_dir.join(format!("signet-daemon-{}", super::target_name()));
        if daemon_named.is_file() {
            return Ok(daemon_named);
        }

        if let Some(bundled) = super::find_bundled_daemon() {
            let src = PathBuf::from(&bundled);
            let target = bin_dir.join("signet-daemon");
            fs::create_dir_all(&bin_dir)?;
            fs::copy(&src, &target)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }

            return Ok(target);
        }

        Err(format!(
            "No daemon binary found at {:?}. \
             Kotlin should have extracted it from APK assets on first launch.",
            daemon_path
        )
        .into())
    }

    fn find_model(&self, data: &std::path::Path, name_prefix: &str) -> Option<PathBuf> {
        let models_dir = data.join("models");
        if !models_dir.is_dir() {
            return None;
        }
        let entries = fs::read_dir(&models_dir).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension() {
                if ext == "gguf" {
                    if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        if fname.to_lowercase().starts_with(name_prefix) {
                            return Some(p);
                        }
                    }
                }
            }
        }
        None
    }

    fn find_any_model(&self, data: &std::path::Path) -> Option<PathBuf> {
        let models_dir = data.join("models");
        if !models_dir.is_dir() {
            return None;
        }
        let entries = fs::read_dir(&models_dir).ok()?;
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension() {
                if ext == "gguf" {
                    return Some(p);
                }
            }
        }
        None
    }

    fn spawn_llama_server(
        &self,
        data: &std::path::Path,
        model: &std::path::Path,
        port: u16,
        extra_args: &[&str],
        log_suffix: &str,
        pid_store: &AtomicU32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let bin_dir = data.join("bin");
        let llama_path = bin_dir.join("llama-server");
        if !llama_path.is_file() {
            log::info!("No llama-server binary, skipping {}", log_suffix);
            return Ok(());
        }

        log::info!(
            "Starting llama-server ({}) with model: {:?}",
            log_suffix,
            model
        );

        let log_path = data.join(format!(".daemon/logs/llama-server-{}.log", log_suffix));
        let log_file = fs::File::create(&log_path)?;

        let mut cmd = Command::new(&llama_path);
        cmd.arg("--model")
            .arg(model)
            .arg("--host")
            .arg("127.0.0.1")
            .arg("--port")
            .arg(port.to_string())
            .arg("--ctx-size")
            .arg("2048")
            .arg("--threads")
            .arg("4");

        for arg in extra_args {
            cmd.arg(arg);
        }

        let child = cmd.stdout(log_file.try_clone()?).stderr(log_file).spawn()?;

        let pid = child.id();
        pid_store.store(pid, Ordering::SeqCst);
        log::info!(
            "llama-server ({}) started PID: {} on port {}",
            log_suffix,
            pid,
            port
        );

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            if std::time::Instant::now() > deadline {
                log::warn!("llama-server ({}) health check timed out", log_suffix);
                break;
            }
            if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
                log::info!("llama-server ({}) health check passed", log_suffix);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }

        Ok(())
    }

    fn start_llama_servers(
        &self,
        data: &std::path::Path,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let embed_model = self.find_model(data, "nomic-embed");
        if let Some(model) = embed_model {
            let _ = self.spawn_llama_server(
                data,
                &model,
                8080,
                &["--embedding"],
                "embed",
                &LLAMA_EMBED_PID,
            );
        } else {
            log::info!("No nomic-embed-text model found, skipping embedding server");
        }

        let llm_model = self
            .find_model(data, "qwen")
            .or_else(|| self.find_model(data, "Qwen"));
        if let Some(model) = llm_model {
            let _ = self.spawn_llama_server(data, &model, 8081, &[], "llm", &LLAMA_LLM_PID);
        } else {
            log::info!("No Qwen extraction model found, skipping LLM server");
        }

        Ok(())
    }

    fn stop_llama_servers(&self) {
        for (label, pid_store) in &[("embed", &LLAMA_EMBED_PID), ("llm", &LLAMA_LLM_PID)] {
            let pid = pid_store.load(Ordering::SeqCst);
            if pid == 0 {
                continue;
            }
            log::info!("Stopping llama-server ({}) PID: {}", label, pid);
            #[cfg(unix)]
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
            for _ in 0..20 {
                let check = unsafe { libc::kill(pid as i32, 0) };
                if check != 0 {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            pid_store.store(0, Ordering::SeqCst);
        }
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
        let data_dir = app_data_dir();

        self.start_llama_servers(&data_dir)?;

        let daemon_path = self.extract_daemon()?;

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
        self.stop_llama_servers();

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
