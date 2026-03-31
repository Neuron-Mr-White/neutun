use std::net::{SocketAddr, ToSocketAddrs};

use super::*;
use structopt::StructOpt;

const HOST_ENV: &'static str = "CTRL_HOST";
const PORT_ENV: &'static str = "CTRL_PORT";
const TLS_OFF_ENV: &'static str = "CTRL_TLS_OFF";

const DEFAULT_HOST: &'static str = "neutun.dev";
const DEFAULT_CONTROL_HOST: &'static str = "wormhole.neutun.dev";
const DEFAULT_CONTROL_PORT: &'static str = "5000";

const SETTINGS_DIR: &'static str = ".neutun";
const SECRET_KEY_FILE: &'static str = "key.token";
const DOMAIN_FILE: &'static str = "domain.txt";
const PORT_FILE: &'static str = "port.txt";

/// Command line arguments
#[derive(Debug, StructOpt)]
pub struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[structopt(short = "v", long = "verbose")]
    pub verbose: bool,

    #[structopt(subcommand)]
    pub command: Option<SubCommand>,

    /// Sets an API authentication key to use for this tunnel
    #[structopt(short = "k", long = "key")]
    pub key: Option<String>,

    /// Specify a sub-domain for this tunnel
    #[structopt(short = "s", long = "subdomain")]
    pub sub_domain: Option<String>,

    /// Specify the domain for this tunnel
    #[structopt(short = "d", long = "domain")]
    pub domain: Option<String>,

    /// Sets the HOST (i.e. localhost) to forward incoming tunnel traffic to
    #[structopt(long = "host", default_value = "localhost")]
    pub local_host: String,

    /// Sets the protocol for local forwarding (i.e. https://localhost) to forward incoming tunnel traffic to
    #[structopt(long = "use-tls", short = "t")]
    pub use_tls: bool,

    /// Sets the port to forward incoming tunnel traffic to on the target host
    #[structopt(short = "p", long = "port", default_value = "8000")]
    pub port: u16,

    /// Sets the address of the local introspection dashboard
    #[structopt(long = "dashboard-port")]
    pub dashboard_port: Option<u16>,

    /// Allow listen to wildcard sub-domains
    #[structopt(short = "w", long = "wildcard")]
    pub wildcard: bool,
}

#[derive(Debug, StructOpt)]
pub enum SubCommand {
    /// Store the API Authentication key
    SetAuth {
        /// Sets an API authentication key on disk for future use
        #[structopt(short = "k", long = "key")]
        key: String,
    },
    /// Interactive onboarding to set up tunnel configuration
    Onboard,
    /// Set the default domain for tunnels
    SetDomain {
        /// The domain to use (e.g., "neutun.dev")
        #[structopt(short = "d", long = "domain")]
        domain: String,
    },
    /// Set the default local port for tunnels
    SetPort {
        /// The port number to forward to
        #[structopt(short = "p", long = "port", default_value = "8000")]
        port: u16,
    },
    /// Run neutun as a background daemon
    Daemon,
    /// List available domains on the server
    Domains,
    /// List currently taken subdomains/wildcards
    TakenDomains,
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
    /// Parse the URL to use to connect to the wormhole control server
    pub fn get() -> Result<Config, ()> {
        // parse the opts
        let opts: Opts = Opts::from_args();

        if opts.verbose {
            std::env::set_var("RUST_LOG", "neutun=debug");
        }

        pretty_env_logger::init();

        let (secret_key, sub_domain) = match opts.command {
            Some(SubCommand::SetAuth { key }) => {
                let key = opts.key.unwrap_or(key);
                let settings_dir = match dirs::home_dir().map(|h| h.join(SETTINGS_DIR)) {
                    Some(path) => path,
                    None => {
                        panic!("Could not find home directory to store token.")
                    }
                };
                std::fs::create_dir_all(&settings_dir)
                    .expect("Fail to create file in home directory");
                std::fs::write(settings_dir.join(SECRET_KEY_FILE), key)
                    .expect("Failed to save authentication key file.");

                eprintln!("Authentication key stored successfully!");
                std::process::exit(0);
            }
            Some(SubCommand::Onboard) => {
                run_onboarding();
                std::process::exit(0);
            }
            Some(SubCommand::SetDomain { domain }) => {
                let domain = opts.domain.unwrap_or(domain);
                let settings_dir = match dirs::home_dir().map(|h| h.join(SETTINGS_DIR)) {
                    Some(path) => path,
                    None => {
                        panic!("Could not find home directory to store settings.")
                    }
                };
                std::fs::create_dir_all(&settings_dir).expect("Fail to create settings directory");
                std::fs::write(settings_dir.join(DOMAIN_FILE), domain)
                    .expect("Failed to save domain file.");
                eprintln!("Domain saved successfully!");
                std::process::exit(0);
            }
            Some(SubCommand::SetPort { port }) => {
                let port = if opts.port != 8000 { opts.port } else { port };
                let settings_dir = match dirs::home_dir().map(|h| h.join(SETTINGS_DIR)) {
                    Some(path) => path,
                    None => {
                        panic!("Could not find home directory to store settings.")
                    }
                };
                std::fs::create_dir_all(&settings_dir).expect("Fail to create settings directory");
                std::fs::write(settings_dir.join(PORT_FILE), port.to_string())
                    .expect("Failed to save port file.");
                eprintln!("Port saved successfully!");
                std::process::exit(0);
            }
            Some(SubCommand::Daemon) => {
                run_daemon();
                std::process::exit(0);
            }
            Some(SubCommand::Domains) => (None, None), // Handled in main
            Some(SubCommand::TakenDomains) => (None, None), // Handled in main
            None => {
                let key = opts.key;
                let sub_domain = opts.sub_domain;
                (
                    match key {
                        Some(key) => Some(key),
                        None => dirs::home_dir()
                            .map(|h| h.join(SETTINGS_DIR).join(SECRET_KEY_FILE))
                            .map(|path| {
                                if path.exists() {
                                    std::fs::read_to_string(path)
                                        .map_err(|e| {
                                            error!("Error reading authentication token: {:?}", e)
                                        })
                                        .ok()
                                } else {
                                    None
                                }
                            })
                            .unwrap_or(None),
                    },
                    sub_domain,
                )
            }
        };

        // Note: For SubCommands like Domains/TakenDomains, we might not need local addr,
        // but it doesn't hurt to parse it if we can, or just default safely if valid commands are used.
        // However, `opts.local_host` has a default, so this should usually pass unless DNS fails.
        let port = opts.port;
        let saved_port = Config::load_saved_port();
        let port = if port == 8000 && saved_port.is_some() {
            saved_port.unwrap()
        } else {
            port
        };
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
                    opts.port
                );
                return Err(());
            }
        };

        // get the host url
        let tls_off = env::var(TLS_OFF_ENV)
            .map(|v| v == "1" || v.to_lowercase() == "true" || v.to_lowercase() == "on")
            .unwrap_or(false);
        let host = env::var(HOST_ENV).unwrap_or(format!("{}", DEFAULT_HOST));

        let control_host = env::var(HOST_ENV).unwrap_or(format!("{}", DEFAULT_CONTROL_HOST));

        let port = env::var(PORT_ENV).unwrap_or(format!("{}", DEFAULT_CONTROL_PORT));

        let scheme = if tls_off { "ws" } else { "wss" };
        let http_scheme = if tls_off { "http" } else { "https" };

        let control_url = format!("{}://{}:{}/wormhole", scheme, control_host, port);
        let control_api_url = format!("{}://{}:{}", http_scheme, control_host, port);

        info!("Control Server URL: {}", &control_url);

        Ok(Config {
            client_id: ClientId::generate(),
            local_host: opts.local_host,
            use_tls: opts.use_tls,
            control_url,
            control_api_url,
            host,
            local_port: opts.port,
            local_addr,
            sub_domain,
            domain: opts.domain.or_else(Config::load_saved_domain),
            dashboard_port: opts.dashboard_port.unwrap_or(0),
            verbose: opts.verbose,
            secret_key: secret_key.map(|s| SecretKey(s)),
            control_tls_off: tls_off,
            first_run: true,
            wildcard: opts.wildcard,
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

    fn get_settings_dir() -> std::path::PathBuf {
        dirs::home_dir()
            .map(|h| h.join(SETTINGS_DIR))
            .expect("Could not find home directory")
    }

    pub fn load_saved_domain() -> Option<String> {
        let path = Self::get_settings_dir().join(DOMAIN_FILE);
        if path.exists() {
            std::fs::read_to_string(path).ok()
        } else {
            None
        }
    }

    pub fn load_saved_port() -> Option<u16> {
        let path = Self::get_settings_dir().join(PORT_FILE);
        if path.exists() {
            std::fs::read_to_string(path)
                .ok()
                .and_then(|s| s.trim().parse().ok())
        } else {
            None
        }
    }
}

fn get_input(prompt: &str, default: &str) -> String {
    print!("{} [{}]: ", prompt, default);
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

fn run_onboarding() {
    let settings_dir = dirs::home_dir()
        .map(|h| h.join(SETTINGS_DIR))
        .expect("Could not find home directory");

    std::fs::create_dir_all(&settings_dir).expect("Failed to create settings directory");

    println!("\n=== Neutun Onboarding ===\n");

    let domain = get_input("Enter your domain", DEFAULT_HOST);
    if domain != DEFAULT_HOST {
        std::fs::write(settings_dir.join(DOMAIN_FILE), &domain).expect("Failed to save domain");
    }

    let port = get_input("Enter your local port", "8000");
    let port: u16 = port.parse().unwrap_or(8000);
    std::fs::write(settings_dir.join(PORT_FILE), port.to_string()).expect("Failed to save port");

    println!("\nNow let's set up your authentication key.");
    println!("You can get your access key from: https://dashboard.neutun.dev\n");

    let key = get_input("Enter your API key (press Enter to skip)", "");
    if !key.is_empty() {
        std::fs::write(settings_dir.join(SECRET_KEY_FILE), &key).expect("Failed to save key");
        println!("\nAPI key saved successfully!");
    } else {
        println!("\nNo API key set. You can add it later with: neutun set-auth -k YOUR_KEY");
    }

    println!("\n=== Onboarding Complete! ===");
    println!("You can now run 'neutun' to start your tunnel.");
    println!("Use 'neutun --help' to see all options.\n");
}

#[cfg(target_os = "windows")]
fn run_daemon() {
    use std::process::Command;

    println!("Starting Neutun as a background service on Windows...");

    let exe_path = std::env::current_exe().expect("Could not get current executable path");

    let child = Command::new("powershell")
        .args(&[
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -WindowStyle Hidden",
                exe_path.display()
            ),
        ])
        .spawn();

    match child {
        Ok(_) => println!("Neutun daemon started successfully in background."),
        Err(e) => eprintln!("Failed to start daemon: {}", e),
    }
}

#[cfg(target_os = "macos")]
fn run_daemon() {
    use std::process::Command;

    println!("Starting Neutun as a background service on macOS...");

    let exe_path = std::env::current_exe().expect("Could not get current executable path");

    let child = Command::new("launchctl")
        .args(&["submit", "-l", "neutun", "--", exe_path.to_str().unwrap()])
        .spawn();

    match child {
        Ok(_) => println!("Neutun daemon started successfully."),
        Err(e) => eprintln!("Failed to start daemon: {}", e),
    }
}

#[cfg(target_os = "linux")]
fn run_daemon() {
    use std::process::Command;

    println!("Starting Neutun as a background service on Linux...");

    let exe_path = std::env::current_exe().expect("Could not get current executable path");

    let child = Command::new("nohup")
        .arg(exe_path)
        .arg(">/dev/null")
        .arg("2>&1")
        .arg("&")
        .spawn();

    match child {
        Ok(_) => println!("Neutun daemon started successfully in background."),
        Err(e) => eprintln!("Failed to start daemon: {}", e),
    }
}
