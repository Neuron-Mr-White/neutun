use colored::Colorize;

use crate::config::{DEFAULT_CONTROL_PORT, DEFAULT_HOST};
use crate::saved_config::{
    is_onboarded, list_sessions, load_config, migrate_legacy_config, save_config, NeutunConfig,
    SessionConfig,
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
    let input = input.trim().to_string();
    if input.is_empty() {
        default.to_string()
    } else {
        input
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

/// Run the onboarding flow. Saves config.json and returns it.
pub fn run_onboarding() -> NeutunConfig {
    println!(
        "\n{}\n",
        "=== Welcome to Neutun! Let's set up your tunnel. ==="
            .green()
            .bold()
    );

    let host = get_input("Enter your domain", DEFAULT_HOST);

    let default_ctrl_host = format!("wormhole.{}", &host);
    let ctrl_host_input = get_input("Enter control server host (optional)", &default_ctrl_host);
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
        "You can get your access key from: https://dashboard.neutun.dev".yellow()
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
        crate::saved_config::get_settings_dir()
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

/// Parameters collected from interactive tunnel start
pub struct InteractiveParams {
    pub port: u16,
    pub subdomain: Option<String>,
    pub domain: Option<String>,
    pub key: Option<String>,
    pub use_tls: bool,
    pub wildcard: bool,
}

/// What the interactive flow decided
pub enum InteractiveResult {
    /// Start a tunnel with these params
    StartTunnel(InteractiveParams),
    /// Restore a saved session (second bool = run as daemon)
    RestoreSession(SessionConfig, bool),
    /// Just finished onboarding — don't start a tunnel
    JustOnboarded,
}

/// Main interactive entry point for bare `neutun` command
pub fn run_interactive() -> InteractiveResult {
    // Check if onboarded
    if !is_onboarded() {
        // Try to migrate legacy files first
        if migrate_legacy_config().is_none() {
            // No legacy files — run full onboarding
            run_onboarding();
            return InteractiveResult::JustOnboarded;
        }
    }

    let config = load_config().expect("Config should exist after onboarding/migration");

    let choice = get_choice(
        "What would you like to do?",
        &[
            "Quick start",
            "Customize and start",
            "Restore saved session",
        ],
    );

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
    })
}

fn run_customize(config: &NeutunConfig) -> InteractiveResult {
    let port_str = get_input("Enter local port", &config.port.to_string());
    let port: u16 = port_str.parse().unwrap_or(config.port);

    let sub_choice = get_choice("Subdomain:", &["Custom", "Random"]);
    let subdomain = match sub_choice {
        0 => {
            let s = get_input("Enter subdomain", "");
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
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
    let use_tls = matches!(tls_input.to_lowercase().as_str(), "yes" | "y");

    let wildcard_input = get_input("Wildcard?", "no");
    let wildcard = matches!(wildcard_input.to_lowercase().as_str(), "yes" | "y");

    InteractiveResult::StartTunnel(InteractiveParams {
        port,
        subdomain,
        domain,
        key,
        use_tls,
        wildcard,
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
        let sub = s.subdomain.as_deref().unwrap_or("(random)");
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
    println!("\nStarting tunnel from '{}'...", session.name.green());

    InteractiveResult::RestoreSession(session, false)
}
