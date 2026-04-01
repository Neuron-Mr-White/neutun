use colored::Colorize;

use crate::saved_config::{
    check_daemon_collision, cleanup_and_list_daemons, is_pid_alive, remove_daemon_entry,
    save_daemon_entry, DaemonEntry,
};

/// Check for collision then spawn a detached child process.
/// Writes a daemon tracking entry on success.
pub fn start_daemon(port: u16, subdomain: &Option<String>, extra_args: Vec<String>) {
    // Check for duplicate daemon (same port + subdomain)
    if let Some(existing) = check_daemon_collision(port, subdomain) {
        let sub_display = existing.subdomain.as_deref().unwrap_or("(random)");
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

    // Strip --daemon / -D from args to avoid infinite recursion
    let args: Vec<String> = extra_args
        .into_iter()
        .filter(|a| a != "--daemon" && a != "-D")
        .collect();

    match spawn_detached(&exe_path, &args) {
        Ok(pid) => {
            let entry = DaemonEntry {
                pid,
                port,
                subdomain: subdomain.clone(),
                started_at: chrono::Local::now().to_rfc3339(),
            };
            save_daemon_entry(&entry);

            let sub_display = subdomain.as_deref().unwrap_or("(random)");
            println!("{} (PID {})", "Neutun daemon started".green().bold(), pid);
            println!("Tunnel: subdomain '{}' -> localhost:{}", sub_display, port);
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

    // Build quoted arg string for PowerShell
    let args_str = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{}\"", a)
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let ps_command = if args_str.is_empty() {
        format!(
            "$p = Start-Process -FilePath '{}' -WindowStyle Hidden -PassThru; $p.Id",
            exe.display()
        )
    } else {
        format!(
            "$p = Start-Process -FilePath '{}' -ArgumentList '{}' -WindowStyle Hidden -PassThru; $p.Id",
            exe.display(),
            args_str
        )
    };

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", &ps_command])
        .output()
        .map_err(|e| format!("{}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    stdout
        .parse::<u32>()
        .map_err(|_| format!("Failed to parse PID from PowerShell output: '{}'", stdout))
}

#[cfg(target_os = "macos")]
fn spawn_detached(exe: &std::path::Path, args: &[String]) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    for arg in args {
        cmd.arg(arg);
    }
    let child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}", e))?;

    Ok(child.id())
}

#[cfg(target_os = "linux")]
fn spawn_detached(exe: &std::path::Path, args: &[String]) -> Result<u32, String> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(exe);
    for arg in args {
        cmd.arg(arg);
    }
    let child = cmd
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}", e))?;

    Ok(child.id())
}

/// List all running daemons (with stale cleanup)
pub fn list_daemons() {
    let daemons = cleanup_and_list_daemons();

    if daemons.is_empty() {
        println!("{}", "No running daemons.".yellow());
        return;
    }

    println!("{}", "Running daemons:".green().bold());
    println!(
        "  {:>8}  {:>6}  {:>20}  {}",
        "PID", "PORT", "SUBDOMAIN", "STARTED"
    );
    println!("  {}", "-".repeat(56));
    for d in &daemons {
        let sub = d.subdomain.as_deref().unwrap_or("(random)");
        println!(
            "  {:>8}  {:>6}  {:>20}  {}",
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
            format!(
                "Daemon with PID {} is not running (cleaned up stale entry).",
                pid
            )
            .yellow()
        );
        return;
    }

    kill_process(pid);
    remove_daemon_entry(pid);
    println!("{}", format!("Daemon with PID {} stopped.", pid).green());
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
        .args(["/PID", &pid.to_string(), "/F"])
        .output();
}

#[cfg(not(target_os = "windows"))]
fn kill_process(pid: u32) {
    use std::process::Command;
    let _ = Command::new("kill").arg(pid.to_string()).output();
}
