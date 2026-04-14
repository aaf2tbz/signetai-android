use super::DaemonManager;

pub struct WindowsManager;

impl DaemonManager for WindowsManager {
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
        let _ = std::fs::remove_file(&pid_path);
        Ok(())
    }

    fn is_running(&self) -> bool {
        false
    }
}
