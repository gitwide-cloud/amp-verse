//! Device-local server identity/configuration. Never saved into shared music.
use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub server_id: String,
    pub station_id: String,
    pub station_name: String,
    pub analysis_enabled: bool,
    pub analysis_workers: usize,
    pub scan_interval_seconds: u64,
    pub pause_analysis_during_radio: bool,
}
impl Config {
    pub fn set_analysis(dir: &Path, enabled: bool) -> Result<(), String> {
        let file = dir.join("server.json");
        let _lock = amp_core::coordination::Lock::acquire(&file)?;
        let mut config: Self =
            serde_json::from_slice(&std::fs::read(&file).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        config.analysis_enabled = enabled;
        amp_core::coordination::atomic_write(&file, &config)
    }
    pub fn load(dir: &Path) -> Result<Self, String> {
        amp_core::coordination::directory(dir)?;
        let file = dir.join("server.json");
        let _lock = amp_core::coordination::Lock::acquire(&file)?;
        let config = match std::fs::read(&file) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes).map_err(|e| e.to_string())?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let c = Self {
                    version: 1,
                    server_id: amp_core::coordination::token(),
                    station_id: amp_core::coordination::token(),
                    station_name: "AMP Radio".into(),
                    analysis_enabled: false,
                    analysis_workers: 1,
                    scan_interval_seconds: 60,
                    pause_analysis_during_radio: true,
                };
                amp_core::coordination::atomic_write(&file, &c)?;
                c
            }
            Err(e) => return Err(e.to_string()),
        };
        if config.version != 1
            || config.server_id.is_empty()
            || config.station_id.is_empty()
            || config.analysis_workers > 32
            || config.scan_interval_seconds < 10
        {
            return Err("Invalid server configuration".into());
        }
        Ok(config)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn servers_keep_independent_persistent_station_ids() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ca = Config::load(a.path()).unwrap();
        let restart = Config::load(a.path()).unwrap();
        let cb = Config::load(b.path()).unwrap();
        assert_eq!(ca.server_id, restart.server_id);
        assert_eq!(ca.station_id, restart.station_id);
        assert_ne!(ca.server_id, cb.server_id);
        assert_ne!(ca.station_id, cb.station_id);
    }
}
