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
mod daemon;
mod error;
mod interactive;
mod introspect;
mod local;
mod saved_config;
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
    // Install the ring CryptoProvider for rustls before any TLS operations.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls CryptoProvider");

    setup_panic!();

    // Parse CLI args once
    let opts = Opts::parse();

    // Handle version flag
    if opts.print_version {
        println!("neutun {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    // Set up logging
    if opts.verbose {
        std::env::set_var("RUST_LOG", "neutun=debug");
    }
    pretty_env_logger::init();

    // Dispatch subcommands
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
            handle_server_action(action).await;
            return;
        }
        None => {}
    }

    // No subcommand: if -p is given start tunnel directly, else interactive mode
    if opts.port.is_some() {
        // Direct tunnel start
        if opts.daemon {
            // Start as daemon — spawn detached child
            let extra_args: Vec<String> = std::env::args().skip(1).collect();
            // Safe: we checked opts.port.is_some() above
            let port = opts.port.expect("port checked above");
            crate::daemon::start_daemon(port, &opts.sub_domain, extra_args);
            return;
        }
        let config = match Config::from_opts(&opts) {
            Ok(c) => c,
            Err(_) => return,
        };
        run_tunnel(config).await;
    } else {
        // Interactive mode
        use crate::interactive::{run_interactive, InteractiveResult};
        match run_interactive() {
            InteractiveResult::StartTunnel(params) => {
                let saved = crate::saved_config::load_config().unwrap_or_default();
                let config = build_config_from_interactive(params, &saved);
                run_tunnel(config).await;
            }
            InteractiveResult::RestoreSession(session, run_as_daemon) => {
                if run_as_daemon {
                    let extra_args: Vec<String> = std::env::args().skip(1).collect();
                    crate::daemon::start_daemon(session.port, &session.subdomain, extra_args);
                    return;
                }
                let config = match Config::from_opts_and_session(&opts, Some(&session)) {
                    Ok(c) => c,
                    Err(_) => return,
                };
                run_tunnel(config).await;
            }
            InteractiveResult::JustOnboarded => {
                // User just finished onboarding — done
            }
        }
    }
}

fn build_config_from_interactive(
    params: crate::interactive::InteractiveParams,
    saved: &crate::saved_config::NeutunConfig,
) -> Config {
    use std::net::ToSocketAddrs;

    let effective_ctrl_host = saved.effective_ctrl_host();
    let tls_off = !saved.tls;
    let scheme = if tls_off { "ws" } else { "wss" };
    let http_scheme = if tls_off { "http" } else { "https" };
    let control_url = format!(
        "{}://{}:{}/wormhole",
        scheme, effective_ctrl_host, saved.ctrl_port
    );
    let control_api_url = format!(
        "{}://{}:{}",
        http_scheme, effective_ctrl_host, saved.ctrl_port
    );

    let local_addr = ("localhost", params.port)
        .to_socket_addrs()
        .unwrap_or(vec![].into_iter())
        .next()
        .expect("Failed to resolve localhost address");

    Config {
        client_id: ClientId::generate(),
        local_host: "localhost".to_string(),
        use_tls: params.use_tls,
        control_url,
        control_api_url,
        host: saved.host.clone(),
        local_port: params.port,
        local_addr,
        sub_domain: params.subdomain,
        domain: params.domain.or(Some(saved.host.clone())),
        dashboard_port: 0,
        verbose: false,
        secret_key: params.key.map(SecretKey),
        control_tls_off: tls_off,
        first_run: true,
        wildcard: params.wildcard,
    }
}

/// Run the tunnel loop. Handles reconnects until fatal error.
async fn run_tunnel(mut config: Config) {
    update::check().await;

    let introspect_dash_addr = introspect::start_introspect_web_dashboard(config.clone()).await;

    loop {
        let (restart_tx, mut restart_rx) = unbounded();
        let wormhole = run_wormhole(config.clone(), introspect_dash_addr, restart_tx);
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
    use crate::saved_config::{load_config, save_config};

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
                    eprintln!(
                        "{}",
                        "Not configured yet. Run 'neutun config onboard' first.".yellow()
                    );
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
            println!(
                "TLS set to: {}",
                if on { "on".green() } else { "off".yellow() }
            );
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

async fn handle_saves_action(action: &SavesAction) {
    use crate::saved_config::{delete_session, list_sessions, load_last_session, load_session, save_session};

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
                println!(
                    "  {}  (port {}, subdomain {})",
                    s.name.cyan(),
                    s.port,
                    sub
                );
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
        SavesAction::Restore { name, daemon: run_as_daemon } => {
            match load_session(name) {
                Some(session) => {
                    println!("Restoring session '{}'...", name.green());
                    if *run_as_daemon {
                        let extra_args: Vec<String> = std::env::args().skip(1).collect();
                        crate::daemon::start_daemon(
                            session.port,
                            &session.subdomain,
                            extra_args,
                        );
                        return;
                    }
                    let opts = Opts::parse();
                    let config = match Config::from_opts_and_session(&opts, Some(&session)) {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    run_tunnel(config).await;
                }
                None => {
                    eprintln!("{}", format!("Session '{}' not found.", name).red());
                }
            }
        }
        SavesAction::Rm { name } => {
            if delete_session(name) {
                println!("Session '{}' deleted.", name.green());
            } else {
                eprintln!("{}", format!("Session '{}' not found.", name).red());
            }
        }
    }
}

fn handle_daemon_action(action: &DaemonAction) {
    match action {
        DaemonAction::Ls => crate::daemon::list_daemons(),
        DaemonAction::Stop { pid } => crate::daemon::stop_daemon(*pid),
        DaemonAction::StopAll => crate::daemon::stop_all_daemons(),
    }
}

async fn handle_server_action(action: &ServerAction) {
    let saved = crate::saved_config::load_config().unwrap_or_default();
    let effective_ctrl_host = saved.effective_ctrl_host();
    let scheme = if saved.tls { "https" } else { "http" };
    let api_url = format!("{}://{}:{}", scheme, effective_ctrl_host, saved.ctrl_port);
    match action {
        ServerAction::Domains => fetch_and_print_domains(&api_url).await,
        ServerAction::Taken => fetch_and_print_taken_domains(&api_url).await,
    }
}

async fn fetch_and_print_domains(api_url: &str) {
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

async fn fetch_and_print_taken_domains(api_url: &str) {
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


/// Setup the tunnel to our control server
async fn run_wormhole(
    config: Config,
    introspect_web_addr: SocketAddr,
    mut restart_tx: UnboundedSender<Option<Error>>,
) -> Result<(), Error> {
    let interface = CliInterface::start(config.clone(), introspect_web_addr);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let Wormhole {
        websocket,
        sub_domain,
        hostname,
    } = connect_to_wormhole(&config).await?;

    // Fetch taken domains for display
    let taken_url = format!("{}/api/taken", config.control_api_url);
    let taken_domains = reqwest::get(&taken_url).await.ok()
        .and_then(|r| futures::executor::block_on(r.json::<Vec<String>>()).ok())
        .map(|v| v.join(", "))
        .unwrap_or_default();

    interface.did_connect(&sub_domain, &hostname, &taken_domains);

    // Save last session after successful connection (used by `neutun saves add`)
    {
        let session = crate::saved_config::SessionConfig {
            name: "last".to_string(),
            port: config.local_port,
            subdomain: config.sub_domain.clone(),
            domain: config.domain.clone().unwrap_or_else(|| config.host.clone()),
            key: config.secret_key.as_ref().map(|k| k.0.clone()),
            use_tls: config.use_tls,
            wildcard: config.wildcard,
            local_host: config.local_host.clone(),
            ctrl_host: None,
            ctrl_port: 0,
            tls: !config.control_tls_off,
            dashboard_port: if config.dashboard_port == 0 {
                None
            } else {
                Some(config.dashboard_port)
            },
        };
        crate::saved_config::save_last_session(&session);
    }

    // split reading and writing
    let (mut ws_sink, mut ws_stream) = websocket.split();

    // tunnel channel
    let (tunnel_tx, mut tunnel_rx) = unbounded::<ControlPacket>();

    // continuously write to websocket tunnel
    let mut restart = restart_tx.clone();
    tokio::spawn(async move {
        loop {
            let packet = match tunnel_rx.next().await {
                Some(data) => data,
                None => {
                    warn!("control flow didn't send anything!");
                    let _ = restart.send(Some(Error::Timeout)).await;
                    return;
                }
            };

            if let Err(e) = ws_sink.send(Message::binary(packet.serialize())).await {
                warn!("failed to write message to tunnel websocket: {:?}", e);
                let _ = restart.send(Some(Error::WebSocketError(e))).await;
                return;
            }
        }
    });

    // continuously read from websocket tunnel

    loop {
        match ws_stream.next().await {
            Some(Ok(message)) if message.is_close() => {
                debug!("got close message");
                let _ = restart_tx.send(None).await;
                return Ok(());
            }
            Some(Ok(message)) => {
                let packet = process_control_flow_message(
                    config.clone(),
                    tunnel_tx.clone(),
                    message.into_data().to_vec(),
                )
                .await
                .map_err(|e| {
                    error!("Malformed protocol control packet: {:?}", e);
                    Error::MalformedMessageFromServer
                })?;
                debug!("Processed packet: {:?}", packet.packet_type());
            }
            Some(Err(e)) => {
                warn!("websocket read error: {:?}", e);
                return Err(Error::Timeout);
            }
            None => {
                warn!("websocket sent none");
                return Err(Error::Timeout);
            }
        }
    }
}

struct Wormhole {
    websocket: WebSocketStream<MaybeTlsStream<TcpStream>>,
    sub_domain: String,
    hostname: String,
}

async fn connect_to_wormhole(config: &Config) -> Result<Wormhole, Error> {
    // Build an explicit rustls TLS connector using ring and webpki-roots.
    // This avoids relying on the default connector, which on Windows can panic
    // if aws-lc-rs fails to initialize.
    let connector = if !config.control_tls_off {
        let mut root_store = rustls::RootCertStore::empty();
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        let tls_config = rustls::ClientConfig::builder()
            .with_root_certificates(root_store)
            .with_no_client_auth();
        Some(Connector::Rustls(Arc::new(tls_config)))
    } else {
        None
    };

    let (mut websocket, _) = tokio_tungstenite::connect_async_tls_with_config(
        &config.control_url,
        None,
        false,
        connector,
    )
    .await?;

    // send our Client Hello message
    let client_hello = match config.secret_key.clone() {
        Some(secret_key) => ClientHello::generate(
            config.sub_domain.clone(),
            config.domain.clone(),
            ClientType::Auth { key: secret_key },
            config.wildcard,
        ),
        None => {
            // if we have a reconnect token, use it.
            if let Some(reconnect) = RECONNECT_TOKEN.lock().await.clone() {
                ClientHello::reconnect(reconnect, config.wildcard)
            } else {
                ClientHello::generate(
                    config.sub_domain.clone(),
                    config.domain.clone(),
                    ClientType::Anonymous,
                    config.wildcard
                )
            }
        }
    };

    info!("connecting to wormhole...");

    let hello = serde_json::to_vec(&client_hello).unwrap();
    websocket
        .send(Message::binary(hello))
        .await
        .expect("Failed to send client hello to wormhole server.");

    // wait for Server hello
    let server_hello_data = websocket
        .next()
        .await
        .ok_or(Error::NoResponseFromServer)??
        .into_data();
    let server_hello = serde_json::from_slice::<ServerHello>(&server_hello_data).map_err(|e| {
        error!("Couldn't parse server_hello from {:?}", e);
        Error::ServerReplyInvalid
    })?;

    let (sub_domain, hostname) = match server_hello {
        ServerHello::Success {
            sub_domain,
            client_id,
            hostname,
        } => {
            info!("Server accepted our connection. I am client_{}", client_id);
            (sub_domain, hostname)
        }
        ServerHello::AuthFailed => {
            return Err(Error::AuthenticationFailed);
        }
        ServerHello::InvalidSubDomain => {
            return Err(Error::InvalidSubDomain);
        }
        ServerHello::SubDomainInUse => {
            return Err(Error::SubDomainInUse);
        }
        ServerHello::Error(error) => return Err(Error::ServerError(error)),
    };

    Ok(Wormhole {
        websocket,
        sub_domain,
        hostname,
    })
}

async fn process_control_flow_message(
    config: Config,
    mut tunnel_tx: UnboundedSender<ControlPacket>,
    payload: Vec<u8>,
) -> Result<ControlPacket, Box<dyn std::error::Error>> {
    let control_packet = ControlPacket::deserialize(&payload)?;

    match &control_packet {
        ControlPacket::Init(stream_id) => {
            info!("stream[{:?}] -> init", stream_id.to_string());
        }
        ControlPacket::Ping(reconnect_token) => {
            log::info!("got ping. reconnect_token={}", reconnect_token.is_some());

            if let Some(reconnect) = reconnect_token {
                let _ = RECONNECT_TOKEN.lock().await.replace(reconnect.clone());
            }
            let _ = tunnel_tx.send(ControlPacket::Ping(None)).await;
        }
        ControlPacket::Refused(_) => return Err("unexpected control packet".into()),
        ControlPacket::End(stream_id) => {
            // find the stream
            let stream_id = stream_id.clone();

            info!("got end stream [{:?}]", &stream_id);

            tokio::spawn(async move {
                let stream = ACTIVE_STREAMS.read().unwrap().get(&stream_id).cloned();
                if let Some(mut tx) = stream {
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    let _ = tx.send(StreamMessage::Close).await.map_err(|e| {
                        error!("failed to send stream close: {:?}", e);
                    });
                    ACTIVE_STREAMS.write().unwrap().remove(&stream_id);
                }
            });
        }
        ControlPacket::Data(stream_id, data) => {
            info!(
                "stream[{:?}] -> new data: {:?}",
                stream_id.to_string(),
                data.len()
            );

            if !ACTIVE_STREAMS.read().unwrap().contains_key(&stream_id) {
                if local::setup_new_stream(config.clone(), tunnel_tx.clone(), stream_id.clone())
                    .await
                    .is_none()
                {
                    error!("failed to open local tunnel")
                }
            }

            // find the right stream
            let active_stream = ACTIVE_STREAMS.read().unwrap().get(&stream_id).cloned();

            // forward data to it
            if let Some(mut tx) = active_stream {
                tx.send(StreamMessage::Data(data.clone())).await?;
                info!("forwarded to local tcp ({})", stream_id.to_string());
            } else {
                error!("got data but no stream to send it to.");
                let _ = tunnel_tx
                    .send(ControlPacket::Refused(stream_id.clone()))
                    .await?;
            }
        }
    };

    Ok(control_packet.clone())
}
