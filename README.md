# Neutun

Neutun is a private-use tool to expose your locally running web server via a public URL.
It is a fork of `tunnelto`, modified for simplified private self-hosting.
The name "Neutun" comes from "tun" which stands for tunnel.

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

    # The base domain for your tunnels (e.g., example.com)
    # This is used for generating the subdomain URL shown to the client.
    TUNNEL_HOST=<YOUR_DOMAIN>

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
    | `TUNNEL_HOST` | The base domain used for constructing tunnel URLs in the handshake. | `neutun.dev` |
    | `MASTER_API_KEY` | The secret key for client authentication. | *(Required)* |
    | `PORT` | The public HTTP port for serving tunnel traffic. | `8080` |
    | `CTRL_PORT` | The port for the control server (WebSockets). | `5000` |
    | `NET_PORT` | Internal port for instance-to-instance gossip. | `6000` |
    | `BLOCKED_SUB_DOMAINS` | Comma-separated list of subdomains to block. | `[]` |
    | `BLOCKED_IPS` | Comma-separated list of IP addresses to block. | `[]` |
    | `MASTER_SIG_KEY` | Key for signing reconnect tokens. Defaults to ephemeral if unset. | *(Ephemeral)* |

    #### Client
    | Variable | Description | Default |
    | :--- | :--- | :--- |
    | `CTRL_HOST` | The hostname of the control server. | `neutun.dev` |
    | `CTRL_PORT` | The port of the control server. **Note:** Defaults to `10001`, but the server defaults to `5000`. Set this to `5000` if using default server config. | `10001` |
    | `CTRL_TLS_OFF` | Set to `1` (or any value) to disable TLS (use `ws://` instead of `wss://`). | `false` |

4.  **Run with Docker Compose:**

    ```bash
    sudo docker compose up -d --build
    ```

    Check the logs to ensure it's running:
    ```bash
    sudo docker compose logs -f
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

## 2. Using the Client

The `neutun` client connects to the server and tunnels traffic from your local machine.

### Usage

1.  **Download or Build the Client:**
    You can build it from source:
    ```bash
    cargo build --release --bin neutun
    # Binary is at ./target/release/neutun
    ```

2.  **Run the Client:**

    You need to tell the client where your server is.

    ```bash
    # Point to your self-hosted server
    export CTRL_HOST="ws.<YOUR_DOMAIN>"  # e.g., ws.example.com (if using the Nginx config above)
    # OR if connecting directly without Nginx proxy for control:
    # export CTRL_HOST="<YOUR_DOMAIN>"
    # export CTRL_PORT=5000

    export CTRL_TLS_OFF=0                # Set to 0 if using HTTPS/WSS (via Nginx), 1 if plain HTTP/WS

    # Run the client
    # -p: Local port to expose (e.g., your web app running on 8000)
    # -k: The MASTER_API_KEY you generated on the server
    # -s: Desired subdomain
    ./neutun -p 8000 -k <YOUR_MASTER_API_KEY> -s myservice
    ```

### Client Help
```
neutun 0.1.19
Neutun Developers
Expose your local web server to the internet with a public url.

USAGE:
    neutun [FLAGS] [OPTIONS] [SUBCOMMAND]

FLAGS:
    -h, --help        Prints help information
    -t, --use-tls     Sets the protocol for local forwarding (i.e. https://localhost) to forward incoming tunnel traffic
                      to
    -V, --version     Prints version information
    -v, --verbose     A level of verbosity, and can be used multiple times
    -w, --wildcard    Allow listen to wildcard sub-domains

OPTIONS:
        --dashboard-port <dashboard-port>    Sets the address of the local introspection dashboard
    -k, --key <key>                          Sets an API authentication key to use for this tunnel
        --host <local-host>                  Sets the HOST (i.e. localhost) to forward incoming tunnel traffic to
                                             [default: localhost]
    -p, --port <port>                        Sets the port to forward incoming tunnel traffic to on the target host
                                             [default: 8000]
    -s, --subdomain <sub-domain>             Specify a sub-domain for this tunnel

SUBCOMMANDS:
    help        Prints this message or the help of the given subcommand(s)
    set-auth    Store the API Authentication key
```

### Subdomains
Subdomains are first-come, first-served. If a subdomain is currently in use by another connected client, you will be unable to claim it until they disconnect.

## License
MIT
