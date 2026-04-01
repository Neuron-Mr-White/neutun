use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::{
    CONFIG_FILE, DAEMONS_DIR, DEFAULT_CONTROL_PORT, DEFAULT_HOST, LAST_SESSION_FILE,
    LEGACY_DOMAIN_FILE, LEGACY_KEY_FILE, LEGACY_PORT_FILE, SAVES_DIR, SETTINGS_DIR,
};

/// Persistent config stored in ~/.neutun/config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeutunConfig {
    pub host: String,
    pub ctrl_host: Option<String>,
    pub ctrl_port: u16,
    pub tls: bool,
    pub port: u16,
    pub key: Option<String>,
}

impl Default for NeutunConfig {
    fn default() -> Self {
        Self {
            host: DEFAULT_HOST.to_string(),
            ctrl_host: None,
            ctrl_port: DEFAULT_CONTROL_PORT,
            tls: true,
            port: 8000,
            key: None,
        }
    }
}

impl NeutunConfig {
    /// Resolve the effective control host.
    /// If ctrl_host is set use it, otherwise derive as wormhole.<host>.
    pub fn effective_ctrl_host(&self) -> String {
        self.ctrl_host
            .clone()
            .unwrap_or_else(|| format!("wormhole.{}", self.host))
    }

    /// Returns a masked display string for the API key.
    pub fn masked_key(&self) -> String {
        match &self.key {
            Some(_) => "****saved****".to_string(),
            None => "(not set)".to_string(),
        }
    }
}

/// Full tunnel session config stored in saves/<name>.json and last_session.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    pub name: String,
    /// Local forwarding port (e.g. 3000)
    pub port: u16,
    pub subdomain: Option<String>,
    /// Base domain for the tunnel (e.g. "neutun.dev")
    pub domain: String,
    pub key: Option<String>,
    /// TLS for local forwarding (--use-tls)
    pub use_tls: bool,
    pub wildcard: bool,
    /// Local hostname to forward to (e.g. "localhost")
    pub local_host: String,
    /// Control server host override (None = derived from domain)
    pub ctrl_host: Option<String>,
    pub ctrl_port: u16,
    /// Control TLS on/off
    pub tls: bool,
    pub dashboard_port: Option<u16>,
}

/// Daemon tracking entry stored in daemons/<pid>.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonEntry {
    pub pid: u32,
    pub port: u16,
    pub subdomain: Option<String>,
    pub started_at: String,
}

// --- Directory helpers ---

pub fn get_settings_dir() -> PathBuf {
    dirs::home_dir()
        .map(|h| h.join(SETTINGS_DIR))
        .expect("Could not find home directory")
}

pub fn get_saves_dir() -> PathBuf {
    get_settings_dir().join(SAVES_DIR)
}

pub fn get_daemons_dir() -> PathBuf {
    get_settings_dir().join(DAEMONS_DIR)
}

fn ensure_dir(path: &PathBuf) {
    fs::create_dir_all(path)
        .unwrap_or_else(|e| panic!("Failed to create directory {:?}: {}", path, e));
}

// --- Config file I/O ---

pub fn load_config() -> Option<NeutunConfig> {
    let path = get_settings_dir().join(CONFIG_FILE);
    if path.exists() {
        let data = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    } else {
        None
    }
}

pub fn save_config(config: &NeutunConfig) {
    let dir = get_settings_dir();
    ensure_dir(&dir);
    let path = dir.join(CONFIG_FILE);
    let data = serde_json::to_string_pretty(config).expect("Failed to serialize config");
    fs::write(&path, data).expect("Failed to write config.json");
}

pub fn is_onboarded() -> bool {
    get_settings_dir().join(CONFIG_FILE).exists()
}

// --- Migration from legacy files ---

/// Migrate from legacy key.token / domain.txt / port.txt to config.json.
/// Returns Some(config) if any legacy files existed and migration was performed.
/// Returns None if no legacy files exist (user is new, needs onboarding).
pub fn migrate_legacy_config() -> Option<NeutunConfig> {
    let dir = get_settings_dir();
    let key_path = dir.join(LEGACY_KEY_FILE);
    let domain_path = dir.join(LEGACY_DOMAIN_FILE);
    let port_path = dir.join(LEGACY_PORT_FILE);

    let has_any = key_path.exists() || domain_path.exists() || port_path.exists();
    if !has_any {
        return None;
    }

    let mut config = NeutunConfig::default();

    if domain_path.exists() {
        if let Ok(domain) = fs::read_to_string(&domain_path) {
            let domain = domain.trim().to_string();
            if !domain.is_empty() {
                config.host = domain;
            }
        }
    }

    if port_path.exists() {
        if let Ok(port_str) = fs::read_to_string(&port_path) {
            if let Ok(port) = port_str.trim().parse::<u16>() {
                config.port = port;
            }
        }
    }

    if key_path.exists() {
        if let Ok(key) = fs::read_to_string(&key_path) {
            let key = key.trim().to_string();
            if !key.is_empty() {
                config.key = Some(key);
            }
        }
    }

    save_config(&config);
    eprintln!("Migrated config from legacy files to config.json");
    Some(config)
}

// --- Session save/load ---

pub fn save_session(session: &SessionConfig) {
    let dir = get_saves_dir();
    ensure_dir(&dir);
    let path = dir.join(format!("{}.json", session.name));
    let data = serde_json::to_string_pretty(session).expect("Failed to serialize session");
    fs::write(&path, data).expect("Failed to write session file");
}

pub fn load_session(name: &str) -> Option<SessionConfig> {
    let path = get_saves_dir().join(format!("{}.json", name));
    if path.exists() {
        let data = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    } else {
        None
    }
}

pub fn list_sessions() -> Vec<SessionConfig> {
    let dir = get_saves_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut sessions = vec![];
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<SessionConfig>(&data) {
                        sessions.push(session);
                    }
                }
            }
        }
    }
    sessions
}

pub fn delete_session(name: &str) -> bool {
    let path = get_saves_dir().join(format!("{}.json", name));
    if path.exists() {
        fs::remove_file(&path).is_ok()
    } else {
        false
    }
}

pub fn save_last_session(session: &SessionConfig) {
    let dir = get_settings_dir();
    ensure_dir(&dir);
    let path = dir.join(LAST_SESSION_FILE);
    let data = serde_json::to_string_pretty(session).expect("Failed to serialize last session");
    fs::write(&path, data).expect("Failed to write last_session.json");
}

pub fn load_last_session() -> Option<SessionConfig> {
    let path = get_settings_dir().join(LAST_SESSION_FILE);
    if path.exists() {
        let data = fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    } else {
        None
    }
}

// --- Daemon tracking ---

pub fn save_daemon_entry(entry: &DaemonEntry) {
    let dir = get_daemons_dir();
    ensure_dir(&dir);
    let path = dir.join(format!("{}.json", entry.pid));
    let data = serde_json::to_string_pretty(entry).expect("Failed to serialize daemon entry");
    fs::write(&path, data).expect("Failed to write daemon entry");
}

pub fn remove_daemon_entry(pid: u32) {
    let path = get_daemons_dir().join(format!("{}.json", pid));
    let _ = fs::remove_file(&path);
}

pub fn list_daemon_entries() -> Vec<DaemonEntry> {
    let dir = get_daemons_dir();
    if !dir.exists() {
        return vec![];
    }
    let mut entries = vec![];
    if let Ok(dir_entries) = fs::read_dir(&dir) {
        for entry in dir_entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(data) = fs::read_to_string(&path) {
                    if let Ok(daemon) = serde_json::from_str::<DaemonEntry>(&data) {
                        entries.push(daemon);
                    }
                }
            }
        }
    }
    entries
}

/// Check if a PID is still alive (platform-specific)
#[cfg(target_os = "windows")]
pub fn is_pid_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output()
        .map(|o| {
            let output = String::from_utf8_lossy(&o.stdout);
            output.contains(&pid.to_string())
        })
        .unwrap_or(false)
}

#[cfg(not(target_os = "windows"))]
pub fn is_pid_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Clean up stale daemon entries and return only live ones
pub fn cleanup_and_list_daemons() -> Vec<DaemonEntry> {
    let entries = list_daemon_entries();
    let mut live = vec![];
    for entry in entries {
        if is_pid_alive(entry.pid) {
            live.push(entry);
        } else {
            remove_daemon_entry(entry.pid);
        }
    }
    live
}

/// Check if a daemon with the same port+subdomain is already running
pub fn check_daemon_collision(port: u16, subdomain: &Option<String>) -> Option<DaemonEntry> {
    let live = cleanup_and_list_daemons();
    live.into_iter()
        .find(|d| d.port == port && &d.subdomain == subdomain)
}
