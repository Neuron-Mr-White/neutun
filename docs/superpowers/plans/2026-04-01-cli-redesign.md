# Neutun CLI Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the neutun client CLI to use namespaced commands, remove env vars, add saved sessions, daemon management, and interactive onboarding.

**Architecture:** Replace the current flat `structopt` CLI definition with a nested subcommand structure using `clap` (derive). Introduce a `config.json`-based config system replacing env vars and individual text files. Add new modules for saves, daemon management, and interactive flows. The runtime `Config` struct stays but its construction is rewritten to source from `config.json` instead of env vars.

**Tech Stack:** Rust, clap 4 (replacing structopt 0.3), serde/serde_json (config files), colored (terminal output), tokio (async runtime)

---

## File Structure

```
neutun/src/
  main.rs              # MODIFY: new CLI dispatch, interactive flow
  config.rs            # REWRITE: new Opts/SubCommand with clap, new Config::get(), 
                       #          remove env vars, add config.json R/W, migration
  saved_config.rs      # CREATE: SavedConfig struct, config.json and saves/*.json I/O
  interactive.rs       # CREATE: onboarding flow, quick start, customize, restore menu
  daemon.rs            # CREATE: daemon start (--daemon flag), ls/stop/stop-all, 
                       #          PID tracking, collision detection
  cli_ui.rs            # MODIFY: minor updates (no functional changes expected)
  error.rs             # MODIFY: add new error variants if needed
  local.rs             # NO CHANGE
  update.rs            # NO CHANGE
  introspect/          # NO CHANGE
```

---

### Task 1: Replace structopt with clap and define new CLI structure

**Files:**
- Modify: `neutun/Cargo.toml`
- Rewrite: `neutun/src/config.rs` (Opts/SubCommand structs only — keep Config struct and impl for now)

This task replaces `structopt` with `clap` derive and defines the full nested subcommand tree. No behavior changes yet — just the argument parsing structure.

- [ ] **Step 1: Update Cargo.toml — replace structopt with clap**

In `neutun/Cargo.toml`, remove the `structopt` line and add `clap`:

```toml
# REMOVE:
# structopt = "0.3.26"

# ADD:
clap = { version = "4", features = ["derive"] }
```

- [ ] **Step 2: Rewrite the Opts and SubCommand structs in config.rs**

Replace the top of `neutun/src/config.rs` with:

```rust
use std::net::{SocketAddr, ToSocketAddrs};

use super::*;
use clap::{Parser, Subcommand};

const DEFAULT_HOST: &str = "neutun.dev";
const DEFAULT_CONTROL_HOST: &str = "wormhole.neutun.dev";
const DEFAULT_CONTROL_PORT: u16 = 5000;

const SETTINGS_DIR: &str = ".neutun";
const CONFIG_FILE: &str = "config.json";
const SAVES_DIR: &str = "saves";
const DAEMONS_DIR: &str = "daemons";
const LAST_SESSION_FILE: &str = "last_session.json";

// Legacy file names (for migration)
const LEGACY_KEY_FILE: &str = "key.token";
const LEGACY_DOMAIN_FILE: &str = "domain.txt";
const LEGACY_PORT_FILE: &str = "port.txt";

/// Neutun: expose your local web server to the internet with a public URL
#[derive(Debug, Parser)]
#[command(name = "neutun", version, about)]
pub struct Opts {
    /// Print version information
    #[arg(short = 'v', long = "version")]
    pub print_version: bool,

    #[command(subcommand)]
    pub command: Option<SubCommand>,

    /// Sets an API authentication key to use for this tunnel
    #[arg(short = 'k', long = "key")]
    pub key: Option<String>,

    /// Specify a sub-domain for this tunnel
    #[arg(short = 's', long = "subdomain")]
    pub sub_domain: Option<String>,

    /// Specify the domain for this tunnel
    #[arg(short = 'd', long = "domain")]
    pub domain: Option<String>,

    /// Sets the HOST (i.e. localhost) to forward incoming tunnel traffic to
    #[arg(long = "host", default_value = "localhost")]
    pub local_host: String,

    /// Sets the protocol for local forwarding (i.e. https://localhost)
    #[arg(long = "use-tls", short = 't')]
    pub use_tls: bool,

    /// Sets the port to forward incoming tunnel traffic to on the target host
    #[arg(short = 'p', long = "port")]
    pub port: Option<u16>,

    /// Sets the address of the local introspection dashboard
    #[arg(long = "dashboard-port")]
    pub dashboard_port: Option<u16>,

    /// Allow listen to wildcard sub-domains
    #[arg(short = 'w', long = "wildcard")]
    pub wildcard: bool,

    /// Run as a background daemon
    #[arg(short = 'D', long = "daemon")]
    pub daemon: bool,

    /// Enable verbose logging
    #[arg(long = "verbose")]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage saved tunnel sessions
    Saves {
        #[command(subcommand)]
        action: SavesAction,
    },
    /// Manage background daemons
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Query the server
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Set the base domain (e.g., neutun.dev)
    Host {
        /// The domain to use
        domain: String,
    },
    /// Set the control server host (optional, derived from host if unset)
    CtrlHost {
        /// The control server hostname
        host: String,
    },
    /// Set the control server port
    CtrlPort {
        /// The control server port number
        port: u16,
    },
    /// Set control TLS on or off
    Tls {
        /// "on" or "off"
        status: String,
    },
    /// Set the default local forwarding port
    Port {
        /// The port number
        port: u16,
    },
    /// Set the API authentication key
    Key {
        /// The API key
        key: String,
    },
    /// Re-run interactive onboarding
    Onboard,
}

#[derive(Debug, Subcommand)]
pub enum SavesAction {
    /// List all saved sessions
    Ls,
    /// Save last tunnel config as a named session
    Add {
        /// Name for the saved session
        name: String,
    },
    /// Start tunnel from a saved session
    Restore {
        /// Name of the saved session
        name: String,
        /// Run as a background daemon
        #[arg(short = 'D', long = "daemon")]
        daemon: bool,
    },
    /// Delete a saved session
    Rm {
        /// Name of the saved session to delete
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// List running daemons
    Ls,
    /// Stop a specific daemon by PID
    Stop {
        /// PID of the daemon to stop
        pid: u32,
    },
    /// Stop all running daemons
    StopAll,
}

#[derive(Debug, Subcommand)]
pub enum ServerAction {
    /// List available domains on the server
    Domains,
    /// List currently taken subdomains
    Taken,
}
```

- [ ] **Step 3: Add a temporary stub for Config::get() to make it compile**

Keep the existing `Config` struct definition as-is. Replace the body of `Config::get()` with a temporary version that just uses clap parsing and hardcoded defaults (no env vars, no file reading). This is so the project compiles — we'll flesh it out in later tasks.

```rust
impl Config {
    pub fn get() -> Result<Config, ()> {
        let opts: Opts = Opts::parse();

        if opts.print_version {
            println!("neutun {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }

        if opts.verbose {
            std::env::set_var("RUST_LOG", "neutun=debug");
        }

        pretty_env_logger::init();

        // Subcommands are handled in main.rs — if we reach here, it's a tunnel run
        let port = opts.port.unwrap_or(8000);
        let local_addr = match (opts.local_host.as_str(), port)
            .to_socket_addrs()
            .unwrap_or(vec![].into_iter())
            .next()
        {
            Some(addr) => addr,
            None => {
                error!(
                    "An invalid local address was specified: {}:{}",
                    opts.local_host.as_str(),
                    port
                );
                return Err(());
            }
        };

        let host = DEFAULT_HOST.to_string();
        let control_host = DEFAULT_CONTROL_HOST.to_string();
        let ctrl_port = DEFAULT_CONTROL_PORT;
        let tls_off = false;

        let scheme = if tls_off { "ws" } else { "wss" };
        let http_scheme = if tls_off { "http" } else { "https" };

        let control_url = format!("{}://{}:{}/wormhole", scheme, control_host, ctrl_port);
        let control_api_url = format!("{}://{}:{}", http_scheme, control_host, ctrl_port);

        info!("Control Server URL: {}", &control_url);

        Ok(Config {
            client_id: ClientId::generate(),
            local_host: opts.local_host,
            use_tls: opts.use_tls,
            control_url,
            control_api_url,
            host,
            local_port: port,
            local_addr,
            sub_domain: opts.sub_domain,
            domain: opts.domain,
            dashboard_port: opts.dashboard_port.unwrap_or(0),
            verbose: opts.verbose,
            secret_key: opts.key.map(|s| SecretKey(s)),
            control_tls_off: tls_off,
            first_run: true,
            wildcard: opts.wildcard,
        })
    }

    pub fn activation_url(&self, full_hostname: &str) -> String {
        format!(
            "{}://{}",
            if self.control_tls_off { "http" } else { "https" },
            full_hostname
        )
    }

    pub fn forward_url(&self) -> String {
        let scheme = if self.use_tls { "https" } else { "http" };
        format!("{}://{}:{}", &scheme, &self.local_host, &self.local_port)
    }

    pub fn ws_forward_url(&self) -> String {
        let scheme = if self.use_tls { "wss" } else { "ws" };
        format!("{}://{}:{}", scheme, &self.local_host, &self.local_port)
    }

    pub fn get_settings_dir() -> std::path::PathBuf {
        dirs::home_dir()
            .map(|h| h.join(SETTINGS_DIR))
            .expect("Could not find home directory")
    }
}
```

Remove the old `get_input`, `run_onboarding`, `run_daemon`, `load_saved_domain`, `load_saved_port` functions and all env var constants (`HOST_ENV`, `PORT_ENV`, `TLS_OFF_ENV`, `SECRET_KEY_FILE`, `DOMAIN_FILE`, `PORT_FILE`). These will be rewritten in new modules.

- [ ] **Step 4: Update main.rs to use clap and new subcommand dispatch**

Replace main.rs to use `clap::Parser` and dispatch subcommands. Temporarily, subcommand handlers will print "not yet implemented":

```rust
use futures::channel::mpsc::{unbounded, UnboundedSender};
use futures::{SinkExt, StreamExt};

use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{Connector, MaybeTlsStream, WebSocketStream};

use human_panic::setup_panic;
pub use log::{debug, error, info, warn};

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

mod cli_ui;
mod config;
mod error;
mod introspect;
mod local;
mod update;
pub use self::error::*;

pub use config::*;
pub use neutun_lib::*;

use crate::cli_ui::CliInterface;
use clap::Parser;
use colored::Colorize;
use futures::future::Either;
use std::time::Duration;
use tokio::sync::Mutex;

pub type ActiveStreams = Arc<RwLock<HashMap<StreamId, UnboundedSender<StreamMessage>>>>;

lazy_static::lazy_static! {
    pub static ref ACTIVE_STREAMS:ActiveStreams = Arc::new(RwLock::new(HashMap::new()));
    pub static ref RECONNECT_TOKEN: Arc<Mutex<Option<ReconnectToken>>> = Arc::new(Mutex::new(None));
}

#[derive(Debug, Clone)]
pub enum StreamMessage {
    Data(Vec<u8>),
    Close,
}

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    setup_panic!();

    let opts = Opts::parse();

    // Handle version flag
    if opts.print_version {
        println!("neutun {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // Handle subcommands
    match &opts.command {
        Some(SubCommand::Config { action }) => {
            handle_config_action(action);
            return;
        }
        Some(SubCommand::Saves { action }) => {
            handle_saves_action(action).await;
            return;
        }
        Some(SubCommand::Daemon { action }) => {
            handle_daemon_action(action);
            return;
        }
        Some(SubCommand::Server { action }) => {
            handle_server_action(action, &opts).await;
            return;
        }
        None => {
            // No subcommand: interactive mode or direct tunnel
        }
    }

    // If we get here, it's a direct tunnel run or interactive mode
    let mut config = match Config::get() {
        Ok(config) => config,
        Err(_) => return,
    };

    update::check().await;

    let introspect_dash_addr = introspect::start_introspect_web_dashboard(config.clone()).await;

    loop {
        let (restart_tx, mut restart_rx) = unbounded();
        let wormhole = run_wormhole(config.clone(), introspect_dash_addr.clone(), restart_tx);
        let result = futures::future::select(Box::pin(wormhole), restart_rx.next()).await;
        config.first_run = false;

        match result {
            Either::Left((Err(e), _)) => match e {
                Error::WebSocketError(_) | Error::NoResponseFromServer | Error::Timeout => {
                    error!("Control error: {:?}. Retrying in 5 seconds.", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Error::AuthenticationFailed => {
                    if config.secret_key.is_none() {
                        eprintln!(
                            ">> {}",
                            "Please use an access key with the `--key` option".yellow()
                        );
                        eprintln!(
                            ">> {}{}",
                            "You can get your access key here: ".yellow(),
                            "https://dashboard.neutun.dev".yellow().underline()
                        );
                    } else {
                        eprintln!(
                            ">> {}{}",
                            "Please check your access key at ".yellow(),
                            "https://dashboard.neutun.dev".yellow().underline()
                        );
                    }
                    eprintln!("\nError: {}", format!("{}", e).red());
                    return;
                }
                _ => {
                    eprintln!("Error: {}", format!("{}", e).red());
                    return;
                }
            },
            Either::Right((Some(e), _)) => {
                warn!("restarting in 3 seconds...from error: {:?}", e);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            _ => {}
        };

        info!("restarting wormhole");
    }
}

fn handle_config_action(action: &ConfigAction) {
    eprintln!("Config action: {:?} — not yet implemented", action);
}

async fn handle_saves_action(action: &SavesAction) {
    eprintln!("Saves action: {:?} — not yet implemented", action);
}

fn handle_daemon_action(action: &DaemonAction) {
    eprintln!("Daemon action: {:?} — not yet implemented", action);
}

async fn handle_server_action(action: &ServerAction, opts: &Opts) {
    // Temporarily build a minimal config for API calls
    let config = match Config::get() {
        Ok(c) => c,
        Err(_) => return,
    };
    match action {
        ServerAction::Domains => fetch_and_print_domains(&config).await,
        ServerAction::Taken => fetch_and_print_taken_domains(&config).await,
    }
}

// ... rest of file (fetch_and_print_domains, fetch_and_print_taken_domains, 
//     run_wormhole, connect_to_wormhole, process_control_flow_message) stays the same
```

Keep `fetch_and_print_domains`, `fetch_and_print_taken_domains`, `run_wormhole`, `connect_to_wormhole`, and `process_control_flow_message` exactly as they are.

- [ ] **Step 5: Remove `use std::env;` from config.rs if present**

Env vars are no longer used. Remove the `use std::env;` import from config.rs (it was used implicitly via `super::*` from main.rs — check both files).

- [ ] **Step 6: Build and fix any compilation errors**

Run: `cargo build 2>&1`

Expected: Successful compilation with no errors. There may be warnings about unused code — that's acceptable at this stage.

- [ ] **Step 7: Verify `-v` shows version**

Run: `cargo run -- -v`

Expected output: `neutun 1.0.6`

- [ ] **Step 8: Verify help output shows new structure**

Run: `cargo run -- --help`

Expected: Shows the new command structure with `config`, `saves`, `daemon`, `server` subcommands.

- [ ] **Step 9: Commit**

```bash
git add -A
git commit -m "refactor: replace structopt with clap, define new namespaced CLI structure"
```

---

### Task 2: Create saved_config.rs — config.json and saves I/O

**Files:**
- Create: `neutun/src/saved_config.rs`
- Modify: `neutun/src/main.rs` (add `mod saved_config;`)

This task creates the config file data structures and I/O functions. No CLI integration yet — just the building blocks.

- [ ] **Step 1: Create saved_config.rs with NeutunConfig struct**

Create `neutun/src/saved_config.rs`:

```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

use crate::config::{
    CONFIG_FILE, DAEMONS_DIR, DEFAULT_CONTROL_HOST, DEFAULT_CONTROL_PORT, DEFAULT_HOST,
    LAST_SESSION_FILE, LEGACY_DOMAIN_FILE, LEGACY_KEY_FILE, LEGACY_PORT_FILE, SAVES_DIR,
    SETTINGS_DIR,
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
    /// If ctrl_host is set, use it. Otherwise derive from host.
    pub fn effective_ctrl_host(&self) -> String {
        self.ctrl_host
            .clone()
            .unwrap_or_else(|| format!("wormhole.{}", self.host))
    }

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
    fs::create_dir_all(path).expect(&format!("Failed to create directory: {:?}", path));
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

/// Check if a PID is still alive
#[cfg(target_os = "windows")]
pub fn is_pid_alive(pid: u32) -> bool {
    use std::process::Command;
    Command::new("tasklist")
        .args(&["/FI", &format!("PID eq {}", pid), "/NH"])
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
        .args(&["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Clean up stale daemon entries and return live ones
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
    live.into_iter().find(|d| d.port == port && d.subdomain == *subdomain)
}
```

- [ ] **Step 2: Add mod declaration in main.rs**

Add after the other mod declarations in main.rs:

```rust
mod saved_config;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build 2>&1`

Expected: Successful compilation. Some warnings about unused functions are acceptable.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add saved_config module with config.json, saves, and daemon I/O"
```

---

### Task 3: Create interactive.rs — onboarding and interactive menus

**Files:**
- Create: `neutun/src/interactive.rs`
- Modify: `neutun/src/main.rs` (add `mod interactive;`)

- [ ] **Step 1: Create interactive.rs**

Create `neutun/src/interactive.rs`:

```rust
use colored::Colorize;

use crate::config::{DEFAULT_CONTROL_PORT, DEFAULT_HOST};
use crate::saved_config::{
    self, is_onboarded, list_sessions, load_config, migrate_legacy_config, save_config,
    NeutunConfig, SessionConfig,
};

fn get_input(prompt: &str, default: &str) -> String {
    if default.is_empty() {
        print!("{}: ", prompt);
    } else {
        print!("{} (press Enter for default) [{}]: ", prompt, default);
    }
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim();
    if input.is_empty() {
        default.to_string()
    } else {
        input.to_string()
    }
}

fn get_choice(prompt: &str, options: &[&str]) -> usize {
    println!("\n{}", prompt);
    for (i, opt) in options.iter().enumerate() {
        println!("  ({}) {}", (b'A' + i as u8) as char, opt);
    }
    print!("\n> ");
    std::io::Write::flush(&mut std::io::stdout()).unwrap();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input).unwrap();
    let input = input.trim().to_uppercase();

    // Support letter input (A, B, C) or number input (1, 2, 3)
    if let Some(c) = input.chars().next() {
        if c.is_ascii_uppercase() {
            let idx = (c as u8 - b'A') as usize;
            if idx < options.len() {
                return idx;
            }
        }
        if let Ok(n) = input.parse::<usize>() {
            if n >= 1 && n <= options.len() {
                return n - 1;
            }
        }
    }

    // Default to first option
    0
}

/// Run the onboarding flow. Returns the saved config.
pub fn run_onboarding() -> NeutunConfig {
    println!("\n{}\n", "=== Welcome to Neutun! Let's set up your tunnel. ===".green().bold());

    let host = get_input("Enter your domain", DEFAULT_HOST);

    let default_ctrl_host = format!("wormhole.{}", &host);
    let ctrl_host_input = get_input(
        "Enter control server host (optional)",
        &default_ctrl_host,
    );
    let ctrl_host = if ctrl_host_input == default_ctrl_host {
        None
    } else {
        Some(ctrl_host_input)
    };

    let ctrl_port_str = get_input(
        "Enter control server port",
        &DEFAULT_CONTROL_PORT.to_string(),
    );
    let ctrl_port: u16 = ctrl_port_str.parse().unwrap_or(DEFAULT_CONTROL_PORT);

    let tls_input = get_input("Control TLS", "on");
    let tls = tls_input.to_lowercase() != "off";

    println!(
        "\n{}",
        "You can get your access key from: https://dashboard.neutun.dev"
            .yellow()
    );
    let key_input = get_input("Enter your API key (press Enter to skip)", "");
    let key = if key_input.is_empty() {
        None
    } else {
        Some(key_input)
    };

    let config = NeutunConfig {
        host,
        ctrl_host,
        ctrl_port,
        tls,
        port: 8000,
        key,
    };

    save_config(&config);

    println!("\n{}", "=== Onboarding Complete! ===".green().bold());
    println!(
        "Config saved to {}",
        saved_config::get_settings_dir()
            .join("config.json")
            .display()
    );
    println!(
        "Run '{}' to start a tunnel, or run '{}' again for interactive mode.\n",
        "neutun -p <port>".cyan(),
        "neutun".cyan()
    );

    config
}

/// Interactive session parameters collected from the user
pub struct InteractiveParams {
    pub port: u16,
    pub subdomain: Option<String>,
    pub domain: Option<String>,
    pub key: Option<String>,
    pub use_tls: bool,
    pub wildcard: bool,
    pub daemon: bool,
}

/// Represents what the interactive menu decided
pub enum InteractiveResult {
    /// Start a tunnel with these params
    StartTunnel(InteractiveParams),
    /// Restore a saved session (with optional daemon flag)
    RestoreSession(SessionConfig, bool),
    /// User just completed onboarding, don't start a tunnel
    JustOnboarded,
}

/// Main interactive entry point for bare `neutun` command
pub fn run_interactive() -> InteractiveResult {
    // Check if onboarded
    if !is_onboarded() {
        // Try migration first
        if migrate_legacy_config().is_none() {
            // No legacy files, run onboarding
            run_onboarding();
            return InteractiveResult::JustOnboarded;
        }
    }

    let config = load_config().expect("Config should exist after onboarding/migration");

    let choice = get_choice("What would you like to do?", &[
        "Quick start",
        "Customize and start",
        "Restore saved session",
    ]);

    match choice {
        0 => run_quick_start(&config),
        1 => run_customize(&config),
        2 => run_restore(),
        _ => unreachable!(),
    }
}

fn run_quick_start(config: &NeutunConfig) -> InteractiveResult {
    let port_str = get_input("Enter local port", &config.port.to_string());
    let port: u16 = port_str.parse().unwrap_or(config.port);

    InteractiveResult::StartTunnel(InteractiveParams {
        port,
        subdomain: None,
        domain: None,
        key: config.key.clone(),
        use_tls: false,
        wildcard: false,
        daemon: false,
    })
}

fn run_customize(config: &NeutunConfig) -> InteractiveResult {
    let port_str = get_input("Enter local port", &config.port.to_string());
    let port: u16 = port_str.parse().unwrap_or(config.port);

    let sub_choice = get_choice("Subdomain:", &["Custom", "Random"]);
    let subdomain = match sub_choice {
        0 => {
            let s = get_input("Enter subdomain", "");
            if s.is_empty() { None } else { Some(s) }
        }
        _ => None, // Random — server assigns one
    };

    let domain_input = get_input("Enter domain", &config.host);
    let domain = if domain_input == config.host {
        None
    } else {
        Some(domain_input)
    };

    let key_input = get_input("Enter API key", &config.masked_key());
    let key = if key_input == config.masked_key() || key_input.is_empty() {
        config.key.clone()
    } else {
        Some(key_input)
    };

    let tls_input = get_input("Use TLS for local forwarding?", "no");
    let use_tls = tls_input.to_lowercase() == "yes" || tls_input.to_lowercase() == "y";

    let wildcard_input = get_input("Wildcard?", "no");
    let wildcard = wildcard_input.to_lowercase() == "yes" || wildcard_input.to_lowercase() == "y";

    InteractiveResult::StartTunnel(InteractiveParams {
        port,
        subdomain,
        domain,
        key,
        use_tls,
        wildcard,
        daemon: false,
    })
}

fn run_restore() -> InteractiveResult {
    let sessions = list_sessions();
    if sessions.is_empty() {
        eprintln!(
            "\n{}",
            "No saved sessions found. Use 'neutun saves add <name>' after starting a tunnel."
                .yellow()
        );
        std::process::exit(0);
    }

    println!("\n{}", "Saved sessions:".bold());
    for (i, s) in sessions.iter().enumerate() {
        let sub = s
            .subdomain
            .as_deref()
            .unwrap_or("(random)");
        println!(
            "  {}. {}    (port {}, subdomain {})",
            i + 1,
            s.name.cyan(),
            s.port,
            sub
        );
    }

    let selection = get_input("Select session", "1");
    let idx: usize = selection.parse().unwrap_or(1);
    let idx = idx.saturating_sub(1).min(sessions.len() - 1);

    let session = sessions[idx].clone();
    println!(
        "\nStarting tunnel from '{}'...",
        session.name.green()
    );

    InteractiveResult::RestoreSession(session, false)
}
```

- [ ] **Step 2: Add mod declaration in main.rs**

Add after the other mod declarations:

```rust
mod interactive;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build 2>&1`

Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add interactive module with onboarding, quick start, customize, and restore flows"
```

---

### Task 4: Create daemon.rs — daemon start, ls, stop, stop-all

**Files:**
- Create: `neutun/src/daemon.rs`
- Modify: `neutun/src/main.rs` (add `mod daemon;`)

- [ ] **Step 1: Create daemon.rs**

Create `neutun/src/daemon.rs`:

```rust
use colored::Colorize;

use crate::saved_config::{
    check_daemon_collision, cleanup_and_list_daemons, is_pid_alive, remove_daemon_entry,
    save_daemon_entry, DaemonEntry,
};

/// Start the current process as a background daemon.
/// This spawns a detached child process with the same arguments (minus --daemon)
/// and writes a tracking entry.
pub fn start_daemon(port: u16, subdomain: &Option<String>, extra_args: Vec<String>) {
    // Check for collision
    if let Some(existing) = check_daemon_collision(port, subdomain) {
        let sub_display = existing
            .subdomain
            .as_deref()
            .unwrap_or("(random)");
        eprintln!(
            "{}",
            format!(
                "Error: A daemon is already running for port {} + subdomain '{}' (PID {})",
                existing.port, sub_display, existing.pid
            )
            .red()
        );
        std::process::exit(1);
    }

    let exe_path = std::env::current_exe().expect("Could not get current executable path");

    // Build args: pass through all args except --daemon / -D
    let args: Vec<String> = extra_args
        .into_iter()
        .filter(|a| a != "--daemon" && a != "-D")
        .collect();

    let child = spawn_detached(&exe_path, &args);

    match child {
        Ok(pid) => {
            let entry = DaemonEntry {
                pid,
                port,
                subdomain: subdomain.clone(),
                started_at: chrono::Local::now().to_rfc3339(),
            };
            save_daemon_entry(&entry);

            let sub_display = subdomain.as_deref().unwrap_or("(random)");
            println!(
                "{} (PID {})",
                "Neutun daemon started".green().bold(),
                pid
            );
            println!(
                "Tunnel: subdomain '{}' -> localhost:{}",
                sub_display, port
            );
        }
        Err(e) => {
            eprintln!("{} {}", "Failed to start daemon:".red(), e);
            std::process::exit(1);
        }
    }
}

#[cfg(target_os = "windows")]
fn spawn_detached(exe: &std::path::Path, args: &[String]) -> Result<u32, String> {
    use std::process::Command;

    let args_str = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("'{}'", a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let child = Command::new("powershell")
        .args(&[
            "-Command",
            &format!(
                "$p = Start-Process -FilePath '{}' -ArgumentList '{}' -WindowStyle Hidden -PassThru; $p.Id",
                exe.display(),
                args_str
            ),
        ])
        .output()
        .map_err(|e| format!("{}", e))?;

    let output = String::from_utf8_lossy(&child.stdout).trim().to_string();
    output.parse::<u32>().map_err(|_| format!("Failed to parse PID from: {}", output))
}

#[cfg(target_os = "macos")]
fn spawn_detached(exe: &std::path::Path, args: &[String]) -> Result<u32, String> {
    use std::process::Command;

    let mut cmd = Command::new(exe);
    for arg in args {
        cmd.arg(arg);
    }

    let child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{}", e))?;

    Ok(child.id())
}

#[cfg(target_os = "linux")]
fn spawn_detached(exe: &std::path::Path, args: &[String]) -> Result<u32, String> {
    use std::process::Command;

    let mut cmd = Command::new(exe);
    for arg in args {
        cmd.arg(arg);
    }

    let child = cmd
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("{}", e))?;

    Ok(child.id())
}

/// List all running daemons
pub fn list_daemons() {
    let daemons = cleanup_and_list_daemons();

    if daemons.is_empty() {
        println!("{}", "No running daemons.".yellow());
        return;
    }

    println!("{}", "Running daemons:".green().bold());
    println!(
        "  {:>8}  {:>6}  {:>15}  {}",
        "PID", "PORT", "SUBDOMAIN", "STARTED"
    );
    println!("  {}", "-".repeat(50));
    for d in &daemons {
        let sub = d.subdomain.as_deref().unwrap_or("(random)");
        println!(
            "  {:>8}  {:>6}  {:>15}  {}",
            d.pid, d.port, sub, d.started_at
        );
    }
}

/// Stop a specific daemon by PID
pub fn stop_daemon(pid: u32) {
    if !is_pid_alive(pid) {
        remove_daemon_entry(pid);
        eprintln!(
            "{}",
            format!("Daemon with PID {} is not running (cleaned up stale entry).", pid).yellow()
        );
        return;
    }

    kill_process(pid);
    remove_daemon_entry(pid);
    println!(
        "{}",
        format!("Daemon with PID {} stopped.", pid).green()
    );
}

/// Stop all running daemons
pub fn stop_all_daemons() {
    let daemons = cleanup_and_list_daemons();

    if daemons.is_empty() {
        println!("{}", "No running daemons to stop.".yellow());
        return;
    }

    for d in &daemons {
        kill_process(d.pid);
        remove_daemon_entry(d.pid);
        println!(
            "{}",
            format!("Stopped daemon PID {} (port {}).", d.pid, d.port).green()
        );
    }
}

#[cfg(target_os = "windows")]
fn kill_process(pid: u32) {
    use std::process::Command;
    let _ = Command::new("taskkill")
        .args(&["/PID", &pid.to_string(), "/F"])
        .output();
}

#[cfg(not(target_os = "windows"))]
fn kill_process(pid: u32) {
    use std::process::Command;
    let _ = Command::new("kill")
        .args(&[&pid.to_string()])
        .output();
}
```

- [ ] **Step 2: Add mod declaration in main.rs**

Add after the other mod declarations:

```rust
mod daemon;
```

- [ ] **Step 3: Build to verify compilation**

Run: `cargo build 2>&1`

Expected: Successful compilation.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat: add daemon module with start, ls, stop, stop-all"
```

---

### Task 5: Wire up Config::get() to read from config.json

**Files:**
- Modify: `neutun/src/config.rs`

This task rewrites `Config::get()` to source values from `config.json` instead of env vars, with CLI flag overrides.

- [ ] **Step 1: Rewrite Config::get() to use NeutunConfig**

Replace the `Config::get()` method in `config.rs` with:

```rust
    pub fn get() -> Result<Config, ()> {
        Self::get_with_overrides(None)
    }

    /// Build a Config from config.json, with optional overrides from a saved session.
    /// CLI flags are applied on top by the caller via the Opts struct.
    pub fn get_with_overrides(session: Option<&crate::saved_config::SessionConfig>) -> Result<Config, ()> {
        let opts: Opts = Opts::parse();

        if opts.print_version {
            println!("neutun {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }

        if opts.verbose {
            std::env::set_var("RUST_LOG", "neutun=debug");
        }

        pretty_env_logger::init();

        // Load base config from config.json
        let saved = crate::saved_config::load_config().unwrap_or_default();

        // Apply session overrides if restoring
        let (host, ctrl_host, ctrl_port, tls, default_port, default_key) = if let Some(s) = session {
            (
                s.domain.clone(),
                s.ctrl_host.clone(),
                s.ctrl_port,
                s.tls,
                s.port,
                s.key.clone(),
            )
        } else {
            (
                saved.host.clone(),
                saved.ctrl_host.clone(),
                saved.ctrl_port,
                saved.tls,
                saved.port,
                saved.key.clone(),
            )
        };

        // CLI flags override everything
        let local_port = opts.port.unwrap_or(if session.is_some() {
            session.unwrap().port
        } else {
            default_port
        });
        let sub_domain = opts.sub_domain.clone().or_else(|| {
            session.and_then(|s| s.subdomain.clone())
        });
        let domain = opts.domain.clone().or_else(|| {
            session.map(|s| s.domain.clone())
        }).or(Some(host.clone()));
        let secret_key = opts.key.clone().or(default_key);
        let use_tls = if session.is_some() { session.unwrap().use_tls || opts.use_tls } else { opts.use_tls };
        let wildcard = if session.is_some() { session.unwrap().wildcard || opts.wildcard } else { opts.wildcard };
        let local_host = if session.is_some() && opts.local_host == "localhost" {
            session.unwrap().local_host.clone()
        } else {
            opts.local_host.clone()
        };

        let local_addr = match (local_host.as_str(), local_port)
            .to_socket_addrs()
            .unwrap_or(vec![].into_iter())
            .next()
        {
            Some(addr) => addr,
            None => {
                error!(
                    "An invalid local address was specified: {}:{}",
                    local_host, local_port
                );
                return Err(());
            }
        };

        let effective_ctrl_host = ctrl_host
            .clone()
            .unwrap_or_else(|| format!("wormhole.{}", &host));
        let tls_off = !tls;

        let scheme = if tls_off { "ws" } else { "wss" };
        let http_scheme = if tls_off { "http" } else { "https" };

        let control_url = format!("{}://{}:{}/wormhole", scheme, effective_ctrl_host, ctrl_port);
        let control_api_url = format!("{}://{}:{}", http_scheme, effective_ctrl_host, ctrl_port);

        info!("Control Server URL: {}", &control_url);

        Ok(Config {
            client_id: ClientId::generate(),
            local_host,
            use_tls,
            control_url,
            control_api_url,
            host,
            local_port,
            local_addr,
            sub_domain,
            domain,
            dashboard_port: opts.dashboard_port.unwrap_or(
                session.and_then(|s| s.dashboard_port).unwrap_or(0)
            ),
            verbose: opts.verbose,
            secret_key: secret_key.map(|s| SecretKey(s)),
            control_tls_off: tls_off,
            first_run: true,
            wildcard,
        })
    }
```

Also add this import at the top of config.rs:

```rust
use clap::Parser;
```

- [ ] **Step 2: Build to verify compilation**

Run: `cargo build 2>&1`

Expected: Successful compilation.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat: wire Config::get() to read from config.json instead of env vars"
```

---

### Task 6: Wire up all subcommand handlers in main.rs

**Files:**
- Modify: `neutun/src/main.rs`

This task replaces the stub handlers with real implementations, wires up the interactive flow, and integrates daemon/saves/config commands.

- [ ] **Step 1: Implement handle_config_action**

Replace the stub `handle_config_action` in main.rs:

```rust
fn handle_config_action(action: &ConfigAction) {
    use crate::saved_config::{is_onboarded, load_config, save_config, NeutunConfig};

    match action {
        ConfigAction::Show => {
            match load_config() {
                Some(c) => {
                    println!("{}", "Current configuration:".green().bold());
                    println!("  Host:         {}", c.host);
                    println!("  Ctrl Host:    {}", c.effective_ctrl_host());
                    println!("  Ctrl Port:    {}", c.ctrl_port);
                    println!("  TLS:          {}", if c.tls { "on" } else { "off" });
                    println!("  Default Port: {}", c.port);
                    println!("  API Key:      {}", c.masked_key());
                }
                None => {
                    eprintln!("{}", "Not configured yet. Run 'neutun config onboard' first.".yellow());
                }
            }
        }
        ConfigAction::Host { domain } => {
            let mut config = load_config().unwrap_or_default();
            config.host = domain.clone();
            save_config(&config);
            println!("Host set to: {}", domain.green());
        }
        ConfigAction::CtrlHost { host } => {
            let mut config = load_config().unwrap_or_default();
            config.ctrl_host = Some(host.clone());
            save_config(&config);
            println!("Control host set to: {}", host.green());
        }
        ConfigAction::CtrlPort { port } => {
            let mut config = load_config().unwrap_or_default();
            config.ctrl_port = *port;
            save_config(&config);
            println!("Control port set to: {}", port.to_string().green());
        }
        ConfigAction::Tls { status } => {
            let mut config = load_config().unwrap_or_default();
            let on = status.to_lowercase() == "on";
            config.tls = on;
            save_config(&config);
            println!("TLS set to: {}", if on { "on".green() } else { "off".yellow() });
        }
        ConfigAction::Port { port } => {
            let mut config = load_config().unwrap_or_default();
            config.port = *port;
            save_config(&config);
            println!("Default local port set to: {}", port.to_string().green());
        }
        ConfigAction::Key { key } => {
            let mut config = load_config().unwrap_or_default();
            config.key = Some(key.clone());
            save_config(&config);
            println!("{}", "API key saved successfully!".green());
        }
        ConfigAction::Onboard => {
            crate::interactive::run_onboarding();
        }
    }
}
```

- [ ] **Step 2: Implement handle_saves_action**

Replace the stub `handle_saves_action`:

```rust
async fn handle_saves_action(action: &SavesAction) {
    use crate::saved_config::{
        delete_session, list_sessions, load_last_session, load_session, save_session,
    };

    match action {
        SavesAction::Ls => {
            let sessions = list_sessions();
            if sessions.is_empty() {
                println!("{}", "No saved sessions.".yellow());
                return;
            }
            println!("{}", "Saved sessions:".green().bold());
            for s in &sessions {
                let sub = s.subdomain.as_deref().unwrap_or("(random)");
                println!("  {}  (port {}, subdomain {})", s.name.cyan(), s.port, sub);
            }
        }
        SavesAction::Add { name } => {
            match load_last_session() {
                Some(mut session) => {
                    session.name = name.clone();
                    save_session(&session);
                    println!("Session '{}' saved successfully!", name.green());
                }
                None => {
                    eprintln!(
                        "{}",
                        "No previous tunnel session found. Start a tunnel first, then save it."
                            .yellow()
                    );
                }
            }
        }
        SavesAction::Restore { name, daemon: run_daemon } => {
            match load_session(name) {
                Some(session) => {
                    println!("Restoring session '{}'...", name.green());
                    if *run_daemon {
                        let extra_args: Vec<String> = std::env::args().skip(1).collect();
                        crate::daemon::start_daemon(session.port, &session.subdomain, extra_args);
                        return;
                    }
                    // Start tunnel with session config
                    let mut config = match Config::get_with_overrides(Some(&session)) {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    run_tunnel(config).await;
                }
                None => {
                    eprintln!(
                        "{}",
                        format!("Session '{}' not found.", name).red()
                    );
                }
            }
        }
        SavesAction::Rm { name } => {
            if delete_session(name) {
                println!("Session '{}' deleted.", name.green());
            } else {
                eprintln!(
                    "{}",
                    format!("Session '{}' not found.", name).red()
                );
            }
        }
    }
}
```

- [ ] **Step 3: Implement handle_daemon_action**

Replace the stub `handle_daemon_action`:

```rust
fn handle_daemon_action(action: &DaemonAction) {
    match action {
        DaemonAction::Ls => {
            crate::daemon::list_daemons();
        }
        DaemonAction::Stop { pid } => {
            crate::daemon::stop_daemon(*pid);
        }
        DaemonAction::StopAll => {
            crate::daemon::stop_all_daemons();
        }
    }
}
```

- [ ] **Step 4: Implement handle_server_action properly**

Replace `handle_server_action` to build config from config.json without needing full Opts parsing:

```rust
async fn handle_server_action(action: &ServerAction, _opts: &Opts) {
    let saved = crate::saved_config::load_config().unwrap_or_default();
    let effective_ctrl_host = saved.effective_ctrl_host();
    let scheme = if saved.tls { "https" } else { "http" };
    let api_url = format!("{}://{}:{}", scheme, effective_ctrl_host, saved.ctrl_port);

    match action {
        ServerAction::Domains => {
            fetch_and_print_domains_url(&api_url).await;
        }
        ServerAction::Taken => {
            fetch_and_print_taken_domains_url(&api_url).await;
        }
    }
}

async fn fetch_and_print_domains_url(api_url: &str) {
    let url = format!("{}/api/domains", api_url);
    match reqwest::get(&url).await {
        Ok(res) => {
            if let Ok(domains) = res.json::<Vec<String>>().await {
                println!("{}", "Available Domains:".green().bold());
                for domain in domains {
                    println!(" - {}", domain);
                }
            } else {
                eprintln!("{}", "Failed to parse domains from server.".red());
            }
        }
        Err(e) => {
            eprintln!("{} {}", "Failed to fetch domains:".red(), e);
        }
    }
}

async fn fetch_and_print_taken_domains_url(api_url: &str) {
    let url = format!("{}/api/taken", api_url);
    match reqwest::get(&url).await {
        Ok(res) => {
            if let Ok(taken) = res.json::<Vec<String>>().await {
                println!("{}", "Taken Subdomains:".yellow().bold());
                for t in taken {
                    println!(" - {}", t);
                }
            } else {
                eprintln!("{}", "Failed to parse taken domains from server.".red());
            }
        }
        Err(e) => {
            eprintln!("{} {}", "Failed to fetch taken domains:".red(), e);
        }
    }
}
```

Remove the old `fetch_and_print_domains` and `fetch_and_print_taken_domains` that took `&Config`.

- [ ] **Step 5: Add the run_tunnel helper and wire up the interactive flow + daemon flag in the None branch**

Add a `run_tunnel` helper and update the `None` (no subcommand) branch in main:

```rust
async fn run_tunnel(mut config: Config) {
    update::check().await;

    let introspect_dash_addr = introspect::start_introspect_web_dashboard(config.clone()).await;

    // Save last session
    {
        let session = crate::saved_config::SessionConfig {
            name: "last".to_string(),
            port: config.local_port,
            subdomain: config.sub_domain.clone(),
            domain: config.domain.clone().unwrap_or_default(),
            key: config.secret_key.as_ref().map(|k| k.0.clone()),
            use_tls: config.use_tls,
            wildcard: config.wildcard,
            host: config.local_host.clone(),
            ctrl_host: None, // derived
            ctrl_port: 0,    // from config.json
            tls: !config.control_tls_off,
            dashboard_port: if config.dashboard_port == 0 { None } else { Some(config.dashboard_port) },
            local_host: config.local_host.clone(),
        };
        crate::saved_config::save_last_session(&session);
    }

    loop {
        let (restart_tx, mut restart_rx) = unbounded();
        let wormhole = run_wormhole(config.clone(), introspect_dash_addr.clone(), restart_tx);
        let result = futures::future::select(Box::pin(wormhole), restart_rx.next()).await;
        config.first_run = false;

        match result {
            Either::Left((Err(e), _)) => match e {
                Error::WebSocketError(_) | Error::NoResponseFromServer | Error::Timeout => {
                    error!("Control error: {:?}. Retrying in 5 seconds.", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
                Error::AuthenticationFailed => {
                    if config.secret_key.is_none() {
                        eprintln!(
                            ">> {}",
                            "Please use an access key with the `--key` option".yellow()
                        );
                        eprintln!(
                            ">> {}{}",
                            "You can get your access key here: ".yellow(),
                            "https://dashboard.neutun.dev".yellow().underline()
                        );
                    } else {
                        eprintln!(
                            ">> {}{}",
                            "Please check your access key at ".yellow(),
                            "https://dashboard.neutun.dev".yellow().underline()
                        );
                    }
                    eprintln!("\nError: {}", format!("{}", e).red());
                    return;
                }
                _ => {
                    eprintln!("Error: {}", format!("{}", e).red());
                    return;
                }
            },
            Either::Right((Some(e), _)) => {
                warn!("restarting in 3 seconds...from error: {:?}", e);
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            _ => {}
        };

        info!("restarting wormhole");
    }
}
```

Update the `None` branch in `main()`:

```rust
        None => {
            // No subcommand: check for direct tunnel flags or interactive mode
        }
    }

    // Check for --daemon flag
    let opts_for_daemon = Opts::parse();

    // If port is specified, it's a direct tunnel run
    if opts_for_daemon.port.is_some() {
        if opts_for_daemon.daemon {
            let extra_args: Vec<String> = std::env::args().skip(1).collect();
            crate::daemon::start_daemon(
                opts_for_daemon.port.unwrap(),
                &opts_for_daemon.sub_domain,
                extra_args,
            );
            return;
        }
        let config = match Config::get() {
            Ok(config) => config,
            Err(_) => return,
        };
        run_tunnel(config).await;
        return;
    }

    // No port, no subcommand: interactive mode
    use crate::interactive::{InteractiveResult, run_interactive};
    match run_interactive() {
        InteractiveResult::StartTunnel(params) => {
            // Build config from interactive params
            let saved = crate::saved_config::load_config().unwrap_or_default();
            let host = params.domain.clone().unwrap_or(saved.host.clone());
            let effective_ctrl_host = saved.effective_ctrl_host();
            let tls_off = !saved.tls;
            let scheme = if tls_off { "ws" } else { "wss" };
            let http_scheme = if tls_off { "http" } else { "https" };
            let control_url = format!("{}://{}:{}/wormhole", scheme, effective_ctrl_host, saved.ctrl_port);
            let control_api_url = format!("{}://{}:{}", http_scheme, effective_ctrl_host, saved.ctrl_port);

            use std::net::ToSocketAddrs;
            let local_addr = match ("localhost", params.port)
                .to_socket_addrs()
                .unwrap_or(vec![].into_iter())
                .next()
            {
                Some(addr) => addr,
                None => {
                    eprintln!("Invalid local address: localhost:{}", params.port);
                    return;
                }
            };

            let config = Config {
                client_id: ClientId::generate(),
                local_host: "localhost".to_string(),
                use_tls: params.use_tls,
                control_url,
                control_api_url,
                host,
                local_port: params.port,
                local_addr,
                sub_domain: params.subdomain,
                domain: params.domain,
                dashboard_port: 0,
                verbose: false,
                secret_key: params.key.map(|s| SecretKey(s)),
                control_tls_off: tls_off,
                first_run: true,
                wildcard: params.wildcard,
            };
            run_tunnel(config).await;
        }
        InteractiveResult::RestoreSession(session, daemon) => {
            if daemon {
                let extra_args: Vec<String> = std::env::args().skip(1).collect();
                crate::daemon::start_daemon(session.port, &session.subdomain, extra_args);
                return;
            }
            let config = match Config::get_with_overrides(Some(&session)) {
                Ok(c) => c,
                Err(_) => return,
            };
            run_tunnel(config).await;
        }
        InteractiveResult::JustOnboarded => {
            // User just finished onboarding, exit gracefully
            return;
        }
    }
```

- [ ] **Step 6: Remove the old duplicate tunnel loop from main**

The old tunnel loop that was directly in `main()` should now be removed since `run_tunnel` handles it. The `main` function should end after the interactive/direct match blocks above.

- [ ] **Step 7: Build and fix compilation errors**

Run: `cargo build 2>&1`

Fix any compilation errors iteratively. Common issues:
- Import paths
- Visibility modifiers (pub)
- Borrowing issues with `Opts::parse()` being called multiple times (fix: parse once at the top of main and pass around)

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat: wire up all subcommand handlers, interactive flow, and daemon support"
```

---

### Task 7: Fix double Opts::parse() and clean up main.rs

**Files:**
- Modify: `neutun/src/main.rs`
- Modify: `neutun/src/config.rs`

The current code calls `Opts::parse()` in multiple places which will cause issues (clap parses args from process, second call gives same result but is wasteful). Refactor so opts are parsed once in main and passed through.

- [ ] **Step 1: Refactor Config::get to accept opts**

Change `Config::get()` and `Config::get_with_overrides()` to accept `&Opts`:

```rust
    pub fn get(opts: &Opts) -> Result<Config, ()> {
        Self::get_with_overrides(opts, None)
    }

    pub fn get_with_overrides(
        opts: &Opts,
        session: Option<&crate::saved_config::SessionConfig>,
    ) -> Result<Config, ()> {
        // Remove Opts::parse() from inside here
        // Use the passed-in opts instead
        // ... rest of implementation stays the same but uses the opts parameter
    }
```

- [ ] **Step 2: Update main.rs to parse opts once and pass everywhere**

At the top of `main()`, parse once:

```rust
    let opts = Opts::parse();
```

Then pass `&opts` to `Config::get(&opts)`, `Config::get_with_overrides(&opts, ...)`, and use `opts` fields directly instead of re-parsing.

- [ ] **Step 3: Build and verify**

Run: `cargo build 2>&1`

Expected: Clean compilation.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor: parse CLI opts once in main and pass through to Config"
```

---

### Task 8: Save last_session.json on successful connection

**Files:**
- Modify: `neutun/src/main.rs` (in `run_wormhole` or `run_tunnel`)

Currently the `save_last_session` call in `run_tunnel` saves before connection succeeds. Move it to after the WebSocket connects.

- [ ] **Step 1: Move last_session save to after successful connection**

In `run_wormhole`, after the `connect_to_wormhole` call succeeds and we have the `sub_domain` and `hostname`, save the session. Pass the necessary data through. The cleanest approach: have `run_wormhole` accept a callback or simply save inside `run_wormhole` after connection succeeds.

Add after the `interface.did_connect(...)` line in `run_wormhole`:

```rust
    // Save last session after successful connection
    {
        let session = crate::saved_config::SessionConfig {
            name: "last".to_string(),
            port: config.local_port,
            subdomain: config.sub_domain.clone(),
            domain: config.domain.clone().unwrap_or_else(|| config.host.clone()),
            key: config.secret_key.as_ref().map(|k| k.0.clone()),
            use_tls: config.use_tls,
            wildcard: config.wildcard,
            host: config.local_host.clone(),
            ctrl_host: None,
            ctrl_port: 0,
            tls: !config.control_tls_off,
            dashboard_port: if config.dashboard_port == 0 { None } else { Some(config.dashboard_port) },
            local_host: config.local_host.clone(),
        };
        crate::saved_config::save_last_session(&session);
    }
```

Remove the earlier `save_last_session` call from `run_tunnel` if it was placed there.

- [ ] **Step 2: Build and verify**

Run: `cargo build 2>&1`

Expected: Clean compilation.

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "fix: save last_session.json only after successful WebSocket connection"
```

---

### Task 9: Clean up warnings, unused code, and final polish

**Files:**
- Modify: multiple files as needed

- [ ] **Step 1: Build with all warnings**

Run: `cargo build 2>&1`

Read all warnings carefully.

- [ ] **Step 2: Fix all warnings**

Common warnings to fix:
- Unused imports (remove `use std::env;` if still present)
- Unused variables (prefix with `_`)
- Dead code (remove or add `#[allow(dead_code)]`)
- Deprecated lifetime elision warnings

- [ ] **Step 3: Build with zero warnings**

Run: `cargo build 2>&1`

Expected: `Finished` with no warnings.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy 2>&1`

Fix any clippy lints.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "chore: clean up warnings and unused code"
```

---

### Task 10: Manual testing against localhost

**Files:** None (testing only)

- [ ] **Step 1: Test version**

Run: `cargo run -- -v`

Expected: `neutun 1.0.6`

- [ ] **Step 2: Test help**

Run: `cargo run -- --help`

Expected: Shows new command structure with config, saves, daemon, server.

- [ ] **Step 3: Test config onboard**

Run: `cargo run -- config onboard`

Walk through the onboarding, entering localhost as domain, 5000 as ctrl port, tls off.

Verify: `~/.neutun/config.json` exists with correct values.

- [ ] **Step 4: Test config show**

Run: `cargo run -- config show`

Expected: Shows the values entered during onboarding.

- [ ] **Step 5: Test config set commands**

Run:
```bash
cargo run -- config port 3000
cargo run -- config key testkey123
cargo run -- config show
```

Expected: Values updated in config.json.

- [ ] **Step 6: Test interactive mode (bare neutun)**

Run: `cargo run`

Expected: Shows the interactive menu (Quick start / Customize / Restore).

- [ ] **Step 7: Test saves workflow**

After running a tunnel:
```bash
cargo run -- saves add test-session
cargo run -- saves ls
cargo run -- saves rm test-session
```

- [ ] **Step 8: Test daemon management**

```bash
cargo run -- daemon ls
```

Expected: "No running daemons."

- [ ] **Step 9: Commit final state**

```bash
git add -A
git commit -m "test: verify CLI redesign works against localhost"
```

---

### Task 11: Update README.md

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Update the Client section of README.md**

Replace the Usage section and Client Help section to reflect the new command structure. Remove all references to environment variables `CTRL_HOST`, `CTRL_PORT`, `CTRL_TLS_OFF` from the client section. Update the client env vars table to state these are removed. Keep the server env vars table as-is.

Update the usage example:

```markdown
### Usage

After installing, run the onboarding wizard:

```bash
neutun config onboard
```

This will prompt you for your domain, control server settings, and API key. Configuration is saved to `~/.neutun/config.json`.

Then start a tunnel:

```bash
neutun -p 8000 -s myservice
```

Or use interactive mode:

```bash
neutun
```
```

Update the Client Help section with the new `--help` output after building.

- [ ] **Step 2: Remove client env var references**

In the Environment Variables Reference section, update the Client table:

```markdown
#### Client

Configuration is managed via `neutun config` commands. Environment variables are no longer used.

| Command | Description | Default |
| :--- | :--- | :--- |
| `neutun config host <domain>` | Set the base domain for tunnels. | `neutun.dev` |
| `neutun config ctrl-host <host>` | Set the control server hostname (optional, derived from host). | `wormhole.<host>` |
| `neutun config ctrl-port <port>` | Set the control server port. | `5000` |
| `neutun config tls <on\|off>` | Enable/disable TLS for the control connection. | `on` |
```

- [ ] **Step 3: Build and capture new help output**

Run: `cargo run -- --help 2>&1`

Paste the output into the README Client Help section.

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs: update README for CLI redesign, remove client env var references"
```
