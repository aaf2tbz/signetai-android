use super::DaemonManager;

pub struct LinuxManager;

impl DaemonManager for LinuxManager {
    fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(bin) = super::find_bundled_daemon() {
            std::process::Command::new(&bin).spawn()?;
            return Ok(());
        }
        Err("No daemon binary found".into())
    }

    fn stop(&self) -> Result<(), Box<dyn std::error::Error>> {
        let data = super::data_dir();
        let pid_path = data.join(".daemon/pid");
        if !pid_path.exists() {
            return Ok(());
        }
        let pid_str = std::fs::read_to_string(&pid_path)?;
        let pid: i32 = pid_str.trim().parse()?;
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
        let _ = std::fs::remove_file(&pid_path);
        Ok(())
    }

    fn is_running(&self) -> bool {
        let data = super::data_dir();
        let pid_path = data.join(".daemon/pid");
        let Ok(content) = std::fs::read_to_string(&pid_path) else {
            return false;
        };
        let Ok(pid) = content.trim().parse::<u32>() else {
            return false;
        };
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}
