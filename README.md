# Neutun

Neutun is a private-use tool to expose your locally running web server via a public URL.
It is a fork of `tunnelto`, modified for simplified private self-hosting.
The name "Neutun" comes from "tun" which stands for tunnel.

## Architecture Overview

Neutun uses two distinct traffic flows. Understanding these is critical for correct firewall and proxy configuration:

```
                         ┌──────────────────────────────────────┐
                         │         Neutun Server (VPS)          │
                         │                                      │
   CLI Client ──wss──────┤►  Control Port (default: 5000)       │
   (your machine)        │   WebSocket control channel          │
                         │   Used for: auth, keepalive,         │
                         │   stream init, tunnel data           │
                         │                                      │
   Browser ──https───────┤►  Public Port (default: 8080)        │
   (end users)           │   HTTP traffic to *.your-domain.com  │
                         │   Routed to the correct tunnel       │
                         └──────────────────────────────────────┘
```

| Port | Protocol | Purpose | Who connects |
|:-----|:---------|:--------|:-------------|
| **5000** (CTRL_PORT) | WebSocket (ws/wss) | Control channel between CLI client and server | Your `neutun` CLI client |
| **8080** (PORT) | HTTP | Public tunnel traffic for `*.your-domain.com` | End-user browsers / API consumers |
| **6000** (NET_PORT) | TCP | Internal gossip (multi-instance only) | Other Neutun server instances |

## 1. Hosting the Server (VPS Guide)

This guide assumes you are using a fresh **Ubuntu 24.04 LTS** VPS.

### Prerequisites

Update your system and install necessary dependencies:

```bash
sudo apt-get update
sudo apt-get install build-essential
sudo apt install -y curl git libssl-dev pkg-config
```

### Install Docker & Docker Compose

We recommend using the official Docker installation instructions:

```bash
# Add Docker's official GPG key:
sudo apt-get update
sudo apt-get install ca-certificates curl
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc

# Add the repository to Apt sources:
echo \
  "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
  $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update

# Install the Docker packages:
sudo apt-get install docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
```

### Deployment

1.  **Clone the Repository:**

    ```bash
    git clone https://github.com/Neuron-Mr-White/neudun.git
    cd neutun
    ```

2.  **Generate a Master API Key:**

    Security is important! Generate a strong random key and save it to a `.env` file. Do not use "supersecret".

    ```bash
    # Generate a random 32-byte hex string
    echo "MASTER_API_KEY=$(openssl rand -hex 32)" >> .env

    # Verify the key was saved
    cat .env
    ```

3.  **Configure Environment Variables:**

    Edit the `.env` file to set your domain and other settings.

    ```bash
    nano .env
    ```

    Add the following content (adjusting `<YOUR_DOMAIN>` to your actual domain, e.g., `example.com`):

    ```env
    # Already added by the command above:
    # MASTER_API_KEY=...

    # Comma-separated list of allowed host domains (e.g., example.com)
    # If the incoming Host header matches one of these exactly, the server returns "Hello World".
    # If the Host header is a subdomain of one of these, it routes to a tunnel.
    ALLOWED_HOSTS=<YOUR_DOMAIN>

    # Ports (Default)
    PORT=8080
    CTRL_PORT=5000
    ```

    ### Environment Variables Reference

    #### Server
    | Variable | Description | Default |
    | :--- | :--- | :--- |
    | `ALLOWED_HOSTS` | Comma-separated list of domains allowed for tunneling. If an exact match, serves "Hello World". | *(Required)* |
    | `MASTER_API_KEY` | The secret key for client authentication. | *(Required)* |
    | `PORT` | The public HTTP port for serving tunnel traffic. | `8080` |
    | `CTRL_PORT` | The port for the control server (WebSockets). | `5000` |
    | `NET_PORT` | Internal port for instance-to-instance gossip. | `6000` |
    | `BLOCKED_SUB_DOMAINS` | Comma-separated list of subdomains to block. | `[]` |
    | `BLOCKED_IPS` | Comma-separated list of IP addresses to block. | `[]` |
    | `MASTER_SIG_KEY` | Key for signing reconnect tokens. Defaults to ephemeral if unset. | *(Ephemeral)* |

    #### Client

    Client configuration is managed via `neutun config` commands and stored in `~/.neutun/config.json`. Environment variables are no longer used.

    | Command | Description | Default |
    | :--- | :--- | :--- |
    | `neutun config host <domain>` | Set the base domain for tunnels. | `neutun.dev` |
    | `neutun config ctrl-host <host>` | Set the control server hostname (optional, derived from host if unset). | `wormhole.<host>` |
    | `neutun config ctrl-port <port>` | Set the control server port. | `5000` |
    | `neutun config tls <on\|off>` | Enable/disable TLS for the control connection. | `on` |
    | `neutun config key <key>` | Set the API authentication key. | *(none)* |

4.  **Run with Docker Compose:**

    ```bash
    sudo docker compose up -d --build
    ```

    Check the logs to ensure it's running:
    ```bash
    sudo docker compose logs -f
    ```

### Firewall / Security Groups (Important!)

If you are running on a cloud provider (AWS, GCP, Azure, etc.), you **must** open the required ports in your cloud firewall (e.g., AWS Security Groups) in addition to any OS-level firewall (`ufw`, `iptables`).

| Port | Direction | Purpose |
|:-----|:----------|:--------|
| **80** | Inbound TCP | HTTP redirect to HTTPS (Nginx) |
| **443** | Inbound TCP | HTTPS for public tunnel traffic + WSS control (Nginx) |
| **5000** | Inbound TCP | **Only if** clients connect directly (without Nginx). Not needed if Nginx proxies the control port. |
| **8080** | Inbound TCP | **Only if** serving public traffic directly (without Nginx). Not needed if Nginx proxies port 8080. |

**Common pitfall:** If port 5000 is blocked by a cloud security group, the client will silently fail with `Error 10060 (TimedOut)` / `Connection timed out`. There is no explicit "connection refused" message because the packets are dropped, not rejected.

If using `ufw` on Ubuntu:
```bash
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp
# Only if NOT using Nginx as reverse proxy:
# sudo ufw allow 5000/tcp
# sudo ufw allow 8080/tcp
```

### Reverse Proxy with Nginx

To serve your tunnels over HTTPS (port 443) and standard HTTP (port 80) without exposing the custom ports directly, use Nginx.

1.  **Install Nginx:**

    ```bash
    sudo apt install -y nginx
    ```

2.  **Configure Nginx:**

    Create a new configuration file:

    ```bash
    sudo nano /etc/nginx/sites-available/neutun
    ```

    Paste the following configuration (replace `<YOUR_DOMAIN>` with your domain, e.g., `example.com`):

    ```nginx
    # Public Tunnel Traffic (HTTP/HTTPS)
    server {
        listen 80;
        server_name <YOUR_DOMAIN> *.<YOUR_DOMAIN>;
        # Redirect all HTTP traffic to HTTPS
        return 301 https://$host$request_uri;
    }

    server {
        listen 443 ssl;
        server_name <YOUR_DOMAIN> *.<YOUR_DOMAIN>;

        # SSL Configuration (Adjust paths to your certificates)
        ssl_certificate /etc/letsencrypt/live/<YOUR_DOMAIN>/fullchain.pem;
        ssl_certificate_key /etc/letsencrypt/live/<YOUR_DOMAIN>/privkey.pem;

        # Forward all traffic to the Neutun server's public port (default 8080)
        location / {
            proxy_pass http://127.0.0.1:8080;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;

            # WebSocket Support (Required for some tunnelled apps)
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
        }
    }

    # Control Server (WebSocket for Client Connection)
    # You can host this on a subdomain like "ws.<YOUR_DOMAIN>" or the same domain.
    # Here is an example using a dedicated subdomain: ws.<YOUR_DOMAIN>
    server {
        listen 443 ssl;
        server_name ws.<YOUR_DOMAIN>; # e.g., ws.example.com

        ssl_certificate /etc/letsencrypt/live/<YOUR_DOMAIN>/fullchain.pem;
        ssl_certificate_key /etc/letsencrypt/live/<YOUR_DOMAIN>/privkey.pem;

        location / {
            # Forward to the Neutun server's control port (default 5000)
            proxy_pass http://127.0.0.1:5000;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
        }
    }
    ```

    **Important Note on 502 Bad Gateway:**
    The Neutun server inside Docker listens on IPv6 `[::]:8080` by default. Nginx on the host might try to connect to IPv4 `127.0.0.1:8080`.
    If you see **502 Bad Gateway** errors:
    1.  Ensure your `docker-compose.yml` maps the ports correctly (e.g., `"8080:8080"`).
    2.  Try changing `proxy_pass http://127.0.0.1:8080;` to `proxy_pass http://localhost:8080;`.
    3.  Or ensure your system handles `localhost` as both IPv4 and IPv6.

3.  **Enable the Site:**

    ```bash
    sudo ln -s /etc/nginx/sites-available/neutun /etc/nginx/sites-enabled/
    sudo nginx -t
    sudo systemctl restart nginx
    ```

4.  **SSL (Optional but Recommended):**

    To support subdomains (e.g., `*.example.com`), you need a wildcard SSL certificate. This requires DNS verification. The following example uses Cloudflare.

    1.  **Install Certbot and Cloudflare Plugin:**

        ```bash
        sudo apt-get install -y certbot python3-certbot-dns-cloudflare
        ```

    2.  **Configure Cloudflare Credentials:**

        Create a credentials file:
        ```bash
        mkdir -p ~/.secrets/certbot
        nano ~/.secrets/certbot/cloudflare.ini
        ```

        Add your API Token (Edit `dns_cloudflare_api_token`):
        ```ini
        dns_cloudflare_api_token = <YOUR_CLOUDFLARE_API_TOKEN>
        ```

        **Cloudflare API Token Requirements:**
        - The token **must** have exactly these permissions:
          - **Zone: Zone: Read**
          - **Zone: DNS: Edit**
        - The token must be scoped to the zone (domain) you are issuing the certificate for.
        - **Do NOT** use the `cfut_` prefixed tokens that some Cloudflare dashboard templates auto-generate. These are "user tokens" with a different format and will cause Certbot to fail with an authentication error. Use a standard API Token created via **My Profile > API Tokens > Create Token**.
        - The credentials file **must** use the key `dns_cloudflare_api_token` (not `dns_cloudflare_api_key`). The older `api_key` + `email` format is deprecated.

        Secure the file:
        ```bash
        chmod 600 ~/.secrets/certbot/cloudflare.ini
        ```

    3.  **Generate the Certificate:**

        ```bash
        sudo certbot certonly \
          --dns-cloudflare \
          --dns-cloudflare-credentials ~/.secrets/certbot/cloudflare.ini \
          -d <YOUR_DOMAIN> \
          -d *.<YOUR_DOMAIN>
        ```

    4.  **Auto-Renewal:**

        Certbot installs a systemd timer for auto-renewal. Verify it's active:
        ```bash
        sudo systemctl status certbot.timer
        ```

        After renewal, Nginx needs to reload. Add a deploy hook:
        ```bash
        sudo nano /etc/letsencrypt/renewal-hooks/deploy/reload-nginx.sh
        ```
        ```bash
        #!/bin/bash
        systemctl reload nginx
        ```
        ```bash
        sudo chmod +x /etc/letsencrypt/renewal-hooks/deploy/reload-nginx.sh
        ```

## 2. Using the Client

The `neutun` client connects to the server and tunnels traffic from your local machine.

### Installation

There are three ways to install the client: using Cargo, downloading a pre-compiled binary, or building from source.

#### Option 1: Install via Cargo

If you have Rust installed, you can install the client directly from crates.io:

```bash
cargo install neutun
```

*Note: The server must be hosted manually using Docker or built from source, as `neutun_server` is not published to crates.io.*

#### Option 2: Pre-compiled Binaries

Download the latest release for your operating system from the [Releases Page](https://github.com/Neuron-Mr-White/neutun/releases/latest).

**Adding to PATH:**

To run `neutun` from any terminal, you should add it to your system's PATH.

*   **Linux / macOS:**
    1.  Download and extract the archive (e.g., `neutun-linux.tar.gz` or `neutun-<version>.bottle.tar.gz`).
    2.  Move the binary to `/usr/local/bin`:
        ```bash
        sudo mv neutun /usr/local/bin/
        sudo chmod +x /usr/local/bin/neutun
        ```
    3.  Verify installation: `neutun -v`

*   **Windows:**
    1.  Download `neutun-windows.exe`.
    2.  Create a folder for your command-line tools (e.g., `C:\Tools`) and move `neutun-windows.exe` there. Rename it to `neutun.exe` for convenience.
    3.  Search for "Edit the system environment variables" in the Start menu.
    4.  Click "Environment Variables", then find the `Path` variable under "System variables" (or "User variables") and click "Edit".
    5.  Click "New" and add the path to your folder (e.g., `C:\Tools`).
    6.  Click OK to save. Open a new Command Prompt or PowerShell and type `neutun -v`.

#### Option 3: Build from Source

```bash
git clone https://github.com/Neuron-Mr-White/neutun.git
cd neutun
cargo build --release --bin neutun
# Binary is at ./target/release/neutun
```

### Usage

First, run the onboarding wizard to configure your connection to the server:

```bash
neutun config onboard
```

This will prompt you for your domain, control server settings, and API key. Configuration is saved to `~/.neutun/config.json`. Environment variables (`CTRL_HOST`, `CTRL_PORT`, `CTRL_TLS_OFF`) are no longer used — all configuration is managed via `neutun config` commands.

Then start a tunnel:

```bash
# Start tunnel on local port 8000
neutun -p 8000

# Start tunnel with specific subdomain and API key
neutun -p 8000 -k <YOUR_MASTER_API_KEY> -s myservice

# Or just run neutun for interactive mode
neutun
```

### Configuration Commands

```bash
neutun config show                   # Show current configuration
neutun config host <domain>          # Set base domain (e.g., example.com)
neutun config ctrl-host <host>       # Set control server host (e.g., ws.example.com)
neutun config ctrl-port <port>       # Set control server port (default: 5000)
neutun config tls <on|off>           # Enable/disable TLS for control connection
neutun config port <port>            # Set default local forwarding port
neutun config key <key>              # Set API authentication key
neutun config onboard                # Re-run interactive onboarding
```

### Saved Sessions

```bash
neutun saves add myapp               # Save last tunnel config as 'myapp'
neutun saves ls                      # List saved sessions
neutun saves restore myapp           # Restore and run 'myapp' session
neutun saves restore myapp --daemon  # Restore 'myapp' as background daemon
neutun saves rm myapp                # Delete saved session
```

### Daemon Management

```bash
neutun -p 8000 -s myapp --daemon     # Start tunnel as background daemon
neutun daemon ls                     # List running daemons
neutun daemon stop <pid>             # Stop a specific daemon
neutun daemon stop-all               # Stop all daemons
```

### Server Info

```bash
neutun server domains                # List available domains
neutun server taken                  # List taken subdomains
```

### Client Help
```
expose your local web server to the internet with a public url

Usage: neutun [OPTIONS] [COMMAND]

Commands:
  config  Manage configuration settings
  saves   Manage saved tunnel profiles
  daemon  Manage background daemon processes
  server  Query server information
  help    Print this message or the help of the given subcommand(s)

Options:
  -v, --version
          Print version information
  -k, --key <KEY>
          Sets an API authentication key to use for this tunnel
  -s, --subdomain <SUB_DOMAIN>
          Specify a sub-domain for this tunnel
  -d, --domain <DOMAIN>
          Specify the domain for this tunnel
      --host <LOCAL_HOST>
          Sets the HOST (i.e. localhost) to forward incoming tunnel traffic to [default: localhost]
  -t, --use-tls
          Sets the protocol for local forwarding (i.e. https://localhost)
  -p, --port <PORT>
          Sets the port to forward incoming tunnel traffic to on the target host
      --dashboard-port <DASHBOARD_PORT>
          Sets the address of the local introspection dashboard
  -w, --wildcard
          Allow listen to wildcard sub-domains
  -D, --daemon
          Run as a background daemon
      --verbose
          Enable verbose/debug logging
  -h, --help
          Print help
```

### Subdomains
Subdomains are first-come, first-served. If a subdomain is currently in use by another connected client, you will be unable to claim it until they disconnect.

## 3. Troubleshooting

### Client crashes with a TLS panic on Windows

**Symptom:** The Windows client immediately panics/crashes when connecting via `wss://`.

**Cause:** Older builds (< v1.0.4) used the `aws-lc-rs` cryptographic backend for `rustls`, which can fail to load its native library on Windows.

**Fix:** Update to v1.0.4 or later (latest: v1.0.6), which uses the `ring` crypto provider and explicitly initializes the TLS stack. If you cannot update immediately, use `CTRL_TLS_OFF=1` to bypass TLS (connect directly to the server's control port without Nginx/SSL).

### Connection times out silently (Error 10060)

**Symptom:** The client hangs and eventually reports a timeout with no clear error message.

**Cause:** The control port (default 5000) is not reachable. This is usually caused by:
1. Cloud firewall / Security Group blocking the port.
2. OS-level firewall (`ufw`, `iptables`) blocking the port.
3. Nginx not configured to proxy the control WebSocket.

**Fix:** Verify the port is open: `nc -zv your-server.com 5000`. If using Nginx, ensure you have the WebSocket `Upgrade` headers in the control server block (see the Nginx config above). If using a cloud provider, check your Security Group / firewall rules.

### 502 Bad Gateway from Nginx

**Symptom:** Browser shows 502 when accessing `subdomain.your-domain.com`.

**Cause:** Nginx cannot reach the Neutun server's backend port. Common reasons:
1. Docker container is not running (`docker compose ps`).
2. IPv4/IPv6 mismatch: Neutun listens on `[::]:8080` (IPv6) but Nginx tries `127.0.0.1:8080` (IPv4).

**Fix:** Try changing `proxy_pass http://127.0.0.1:8080;` to `proxy_pass http://localhost:8080;` in your Nginx config, or use `proxy_pass http://[::1]:8080;`.

### Version update loop ("New version available: X => vY")

**Symptom:** The client always reports a new version is available even after updating.

**Cause:** In builds prior to v1.0.4, the Cargo.toml version (`0.1.x`) did not match the GitHub release tag (`v1.0.x`), causing the semver comparison to always report an update.

**Fix:** Update to v1.0.4 or later (latest: v1.0.6), where the versions are synchronized.

## License
MIT
