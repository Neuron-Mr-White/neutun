use std::net::{SocketAddr, ToSocketAddrs};

use super::*;
use clap::{Parser, Subcommand};

pub(crate) const DEFAULT_HOST: &str = "neutun.dev";
#[allow(dead_code)]
pub(crate) const DEFAULT_CONTROL_HOST: &str = "wormhole.neutun.dev";
pub(crate) const DEFAULT_CONTROL_PORT: u16 = 5000;

pub(crate) const SETTINGS_DIR: &str = ".neutun";
pub(crate) const CONFIG_FILE: &str = "config.json";
pub(crate) const SAVES_DIR: &str = "saves";
pub(crate) const DAEMONS_DIR: &str = "daemons";
pub(crate) const LAST_SESSION_FILE: &str = "last_session.json";
pub(crate) const LEGACY_KEY_FILE: &str = "key.token";
pub(crate) const LEGACY_DOMAIN_FILE: &str = "domain.txt";
pub(crate) const LEGACY_PORT_FILE: &str = "port.txt";

/// Command line arguments
#[derive(Debug, Parser)]
#[command(name = "neutun", version, about, disable_version_flag = true)]
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
    #[arg(short = 't', long = "use-tls")]
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

    /// Enable verbose/debug logging
    #[arg(long = "verbose")]
    pub verbose: bool,
}

#[derive(Debug, Subcommand)]
pub enum SubCommand {
    /// Manage configuration settings
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage saved tunnel profiles
    Saves {
        #[command(subcommand)]
        action: SavesAction,
    },
    /// Manage background daemon processes
    Daemon {
        #[command(subcommand)]
        action: DaemonAction,
    },
    /// Query server information
    Server {
        #[command(subcommand)]
        action: ServerAction,
    },
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Show current configuration
    Show,
    /// Set the default host domain
    Host {
        /// The domain to use (e.g., "neutun.dev")
        domain: String,
    },
    /// Set the control server host
    CtrlHost {
        /// The control server hostname
        host: String,
    },
    /// Set the control server port
    CtrlPort {
        /// The control server port number
        port: u16,
    },
    /// Enable or disable TLS
    Tls {
        /// "on" or "off"
        status: String,
    },
    /// Set the default local port
    Port {
        /// The port number to forward to
        port: u16,
    },
    /// Set the API authentication key
    Key {
        /// The API key
        key: String,
    },
    /// Interactive onboarding wizard
    Onboard,
}

#[derive(Debug, Subcommand)]
pub enum SavesAction {
    /// List saved tunnel profiles
    Ls,
    /// Save the current tunnel profile
    Add {
        /// Name for the saved profile
        name: String,
    },
    /// Restore a saved tunnel profile
    Restore {
        /// Name of the profile to restore
        name: String,
        /// Run as daemon after restoring
        #[arg(short = 'D', long = "daemon")]
        daemon: bool,
    },
    /// Remove a saved tunnel profile
    Rm {
        /// Name of the profile to remove
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonAction {
    /// List running daemon processes
    Ls,
    /// Stop a specific daemon by PID
    Stop {
        /// Process ID of the daemon to stop
        pid: u32,
    },
    /// Stop all running daemons
    StopAll,
}

#[derive(Debug, Subcommand)]
pub enum ServerAction {
    /// List available domains on the server
    Domains,
    /// List currently taken subdomains/wildcards
    Taken,
}

/// Config
#[derive(Debug, Clone)]
pub struct Config {
    pub client_id: ClientId,
    pub control_url: String,
    pub control_api_url: String,
    pub use_tls: bool,
    pub host: String,
    pub local_host: String,
    pub local_port: u16,
    pub local_addr: SocketAddr,
    pub sub_domain: Option<String>,
    pub domain: Option<String>,
    pub secret_key: Option<SecretKey>,
    pub control_tls_off: bool,
    pub first_run: bool,
    pub dashboard_port: u16,
    pub verbose: bool,
    pub wildcard: bool,
}

impl Config {
    /// Build Config from parsed opts + config.json (no env vars).
    /// CLI flags override config.json values.
    #[allow(clippy::result_unit_err)]
    pub fn from_opts(opts: &Opts) -> Result<Config, ()> {
        Self::from_opts_and_session(opts, None)
    }

    /// Build Config from opts with an optional saved session override.
    /// Priority: CLI flags > session config > config.json defaults.
    #[allow(clippy::result_unit_err)]
    pub fn from_opts_and_session(
        opts: &Opts,
        session: Option<&crate::saved_config::SessionConfig>,
    ) -> Result<Config, ()> {
        // Load config.json (falls back to defaults if not present)
        let saved = crate::saved_config::load_config().unwrap_or_default();

        // Resolve local port: CLI > session > config.json default
        let port = opts
            .port
            .unwrap_or_else(|| session.map(|s| s.port).unwrap_or(saved.port));

        // Resolve local host: CLI (non-default) > session > "localhost"
        let local_host = if opts.local_host != "localhost" {
            opts.local_host.clone()
        } else if let Some(s) = session {
            s.local_host.clone()
        } else {
            opts.local_host.clone()
        };

        let local_addr = match (local_host.as_str(), port)
            .to_socket_addrs()
            .unwrap_or(vec![].into_iter())
            .next()
        {
            Some(addr) => addr,
            None => {
                error!(
                    "An invalid local address was specified: {}:{}",
                    local_host, port,
                );
                return Err(());
            }
        };

        // Resolve control settings: session > config.json
        let (ctrl_host_opt, ctrl_port, tls) = if let Some(s) = session {
            (s.ctrl_host.clone(), s.ctrl_port, s.tls)
        } else {
            (saved.ctrl_host.clone(), saved.ctrl_port, saved.tls)
        };

        let effective_ctrl_host =
            ctrl_host_opt.unwrap_or_else(|| format!("wormhole.{}", saved.host));
        let tls_off = !tls;

        let scheme = if tls_off { "ws" } else { "wss" };
        let http_scheme = if tls_off { "http" } else { "https" };

        let control_url = format!(
            "{}://{}:{}/wormhole",
            scheme, effective_ctrl_host, ctrl_port
        );
        let control_api_url = format!("{}://{}:{}", http_scheme, effective_ctrl_host, ctrl_port);

        info!("Control Server URL: {}", &control_url);

        // Resolve tunnel-specific fields: CLI > session > saved key
        let sub_domain = opts
            .sub_domain
            .clone()
            .or_else(|| session.and_then(|s| s.subdomain.clone()));

        let domain = opts
            .domain
            .clone()
            .or_else(|| session.map(|s| s.domain.clone()));

        let secret_key = opts
            .key
            .clone()
            .or_else(|| session.and_then(|s| s.key.clone()))
            .or_else(|| saved.key.clone());

        let use_tls = opts.use_tls || session.map(|s| s.use_tls).unwrap_or(false);
        let wildcard = opts.wildcard || session.map(|s| s.wildcard).unwrap_or(false);

        let dashboard_port = opts
            .dashboard_port
            .unwrap_or_else(|| session.and_then(|s| s.dashboard_port).unwrap_or(0));

        Ok(Config {
            client_id: ClientId::generate(),
            local_host,
            use_tls,
            control_url,
            control_api_url,
            host: saved.host.clone(),
            local_port: port,
            local_addr,
            sub_domain,
            domain,
            dashboard_port,
            verbose: opts.verbose,
            secret_key: secret_key.map(SecretKey),
            control_tls_off: tls_off,
            first_run: true,
            wildcard,
        })
    }

    pub fn activation_url(&self, full_hostname: &str) -> String {
        format!(
            "{}://{}",
            if self.control_tls_off {
                "http"
            } else {
                "https"
            },
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
