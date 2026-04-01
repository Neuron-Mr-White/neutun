# Neutun CLI Redesign

Date: 2026-04-01

## Summary

Redesign the neutun client CLI to use consistent namespaced commands, remove all environment variable configuration, introduce saved sessions, daemon management, and an interactive onboarding/start flow.

## Goals

1. Remove all environment variables (`CTRL_HOST`, `CTRL_PORT`, `CTRL_TLS_OFF`) from the client. Configuration is stored in `~/.neutun/config.json`, written exclusively via CLI commands.
2. Fix `-v` to show version (currently mapped to `--verbose`).
3. Restructure all commands under 4 namespaces: `config`, `saves`, `daemon`, `server`.
4. Add interactive mode when running bare `neutun`.
5. Add saved sessions feature for full tunnel config snapshots.
6. Add daemon management commands.
7. Compile with zero warnings and test against localhost.

## Command Structure

### Root command

```
neutun                                # Interactive mode (onboard or start)
neutun -v, --version                  # Show version
neutun -p <port> [flags]              # Start tunnel directly
```

### Runtime flags (tunnel start)

```
-p, --port <port>                     # Local port to forward to
-s, --subdomain <subdomain>           # Request specific subdomain
-d, --domain <domain>                 # Override domain for this session
-k, --key <key>                       # Override API key for this session
-t, --use-tls                         # Use TLS for local forwarding
-w, --wildcard                        # Listen on wildcard subdomains
-D, --daemon                          # Run as background daemon
--host <host>                         # Local host (default: localhost)
--dashboard-port <port>               # Introspection dashboard port
--verbose                             # Verbose logging (no short alias)
```

### `neutun config` namespace

```
neutun config show                    # Print all current config values
neutun config host <domain>           # Set base domain (e.g., neutun.dev)
neutun config ctrl-host <host>        # Set control host (optional, derived from host if unset)
neutun config ctrl-port <port>        # Set control server port (default: 5000)
neutun config tls <on|off>            # Set control TLS on/off (default: on)
neutun config port <port>             # Set default local forwarding port
neutun config key <key>               # Set API authentication key
neutun config onboard                 # Re-run full interactive onboarding
```

### `neutun saves` namespace

```
neutun saves ls                       # List all saved sessions
neutun saves add <name>               # Save last tunnel config as named session
neutun saves restore <name>           # Start tunnel from saved session
neutun saves restore <name> --daemon  # Start saved session as daemon
neutun saves rm <name>                # Delete saved session
```

### `neutun daemon` namespace

```
neutun daemon ls                      # List running daemons
neutun daemon stop <pid>              # Stop a specific daemon
neutun daemon stop-all                # Stop all daemons
```

### `neutun server` namespace

```
neutun server domains                 # List available domains
neutun server taken                   # List taken subdomains
```

## Breaking Changes

| Old | New | Notes |
|-----|-----|-------|
| `neutun set-auth -k KEY` | `neutun config key KEY` | |
| `neutun set-domain -d DOM` | `neutun config host DOM` | |
| `neutun set-port -p PORT` | `neutun config port PORT` | Sets local forwarding port default |
| `neutun onboard` | `neutun config onboard` | Also triggered by bare `neutun` if not onboarded |
| `neutun daemon` (subcommand) | `neutun -p PORT --daemon` | Daemon is now a flag, `neutun daemon` is management |
| `neutun domains` | `neutun server domains` | |
| `neutun taken-domains` | `neutun server taken` | |
| `-v` (verbose) | `--verbose` only | `-v` is now `--version` |
| `-V` (version) | `-v, --version` | |
| `CTRL_HOST` env var | `neutun config host` / `neutun config ctrl-host` | Env var removed |
| `CTRL_PORT` env var | `neutun config ctrl-port` | Env var removed |
| `CTRL_TLS_OFF` env var | `neutun config tls <on\|off>` | Env var removed |

## Config Resolution Order

```
CLI flags (highest)  >  Saved session (via restore)  >  config.json (lowest)
```

- `neutun -p 4000` -- port 4000, everything else from config.json
- `neutun saves restore myapp` -- everything from save file
- `neutun saves restore myapp -p 9000` -- save file values, port overridden to 9000

Control server settings (ctrl_host, ctrl_port, tls) come from config.json unless a save overrides them. There are no CLI flags for these -- they are set via `neutun config` commands only.

## Storage Layout

```
~/.neutun/
  config.json              # Main config (replaces individual .txt files and env vars)
  saves/
    <name>.json            # Saved tunnel sessions
  daemons/
    <pid>.json             # Tracking files for running daemons
  key.token                # DEPRECATED: migrated to config.json on first run
  domain.txt               # DEPRECATED: migrated to config.json on first run
  port.txt                 # DEPRECATED: migrated to config.json on first run
  last_session.json        # Last successfully connected tunnel config
```

### config.json

```json
{
  "host": "neutun.dev",
  "ctrl_host": null,
  "ctrl_port": 5000,
  "tls": true,
  "port": 8000,
  "key": "YOUR_API_KEY"
}
```

When `ctrl_host` is `null`, the client derives it as `wormhole.<host>`.

Default values when saving without input:
- `host`: `"neutun.dev"`
- `ctrl_host`: `null` (derived)
- `ctrl_port`: `5000`
- `tls`: `true`
- `port`: `8000`
- `key`: `null`

### saves/<name>.json

```json
{
  "name": "myapp",
  "port": 3000,
  "subdomain": "myapp",
  "domain": "neutun.dev",
  "key": "YOUR_API_KEY",
  "use_tls": false,
  "wildcard": false,
  "local_host": "localhost",
  "ctrl_host": null,
  "ctrl_port": 5000,
  "tls": true,
  "dashboard_port": null
}
```

### daemons/<pid>.json

```json
{
  "pid": 12345,
  "port": 3000,
  "subdomain": "myapp",
  "started_at": "2026-04-01T12:00:00Z"
}
```

### last_session.json

Same schema as saves. Written when a tunnel connects successfully. Read by `neutun saves add <name>`.

## Interactive Flow

### Path 1: Not onboarded (no config.json)

```
Welcome to Neutun! Let's set up your tunnel.

Enter your domain (press Enter for default) [neutun.dev]:
Enter control server host (optional, press Enter for default) [wormhole.neutun.dev]:
Enter control server port (press Enter for default) [5000]:
Control TLS (press Enter for default) [on]:
Enter your API key (press Enter to skip):

Onboarding complete! Config saved to ~/.neutun/config.json
Run 'neutun -p <port>' to start a tunnel, or run 'neutun' again for interactive mode.
```

### Path 2: Already onboarded

```
What would you like to do?
  (A) Quick start
  (B) Customize and start
  (C) Restore saved session

> A
Enter local port (press Enter for default) [8000]: 3000
Starting tunnel on port 3000...
```

```
> B
Enter local port (press Enter for default) [8000]: 3000
Subdomain: (A) Custom  (B) Random
> A
Enter subdomain: myapp
Enter domain (press Enter for default) [neutun.dev]:
Enter API key (press Enter for default) [****saved****]:
Use TLS for local forwarding? (press Enter for default) [no]:
Wildcard? (press Enter for default) [no]:
Starting tunnel...
```

```
> C
Saved sessions:
  1. myapp-dev    (port 3000, subdomain myapp)
  2. api-staging  (port 8080, subdomain api)
Select session: 1
Starting tunnel from 'myapp-dev'...
```

If no saved sessions exist:
```
No saved sessions found. Use 'neutun saves add <name>' after starting a tunnel.
```

Every prompt shows the default in brackets. Pressing Enter without typing selects the default. The API key is always masked in display.

## Saves Lifecycle

1. User starts a tunnel (any method: flags, interactive, restore).
2. On successful WebSocket connection, the client writes `~/.neutun/last_session.json` with the full tunnel config.
3. User stops tunnel (Ctrl+C or daemon stop).
4. User runs `neutun saves add <name>` -- reads `last_session.json`, writes to `~/.neutun/saves/<name>.json`.
5. `neutun saves restore <name>` reads the save file and starts the tunnel with those values. CLI flags can override individual values.

## Daemon Behavior

### Starting

```
$ neutun -p 3000 -s myapp --daemon
Neutun daemon started (PID 12345)
Tunnel: myapp.neutun.dev -> localhost:3000
```

Creates `~/.neutun/daemons/12345.json` with pid, port, subdomain, and start time.

### Collision detection

Before starting, scan `~/.neutun/daemons/` for entries with matching port+subdomain. Verify each PID is still alive (clean up stale entries). If a live match is found:

```
Error: A daemon is already running for port 3000 + subdomain 'myapp' (PID 12345)
```

Different tunnels (different port or subdomain) are allowed to run concurrently.

The same collision detection applies when using `neutun saves restore <name> --daemon`.

### Management

- `neutun daemon ls` -- list all running daemons (with stale cleanup)
- `neutun daemon stop <pid>` -- send SIGTERM/kill to PID, remove tracking file
- `neutun daemon stop-all` -- stop all tracked daemons

### Cleanup

On daemon exit (normal or crash), the tracking file should be cleaned up. On startup of any daemon command, stale PIDs (process no longer alive) are cleaned automatically.

## Version Display

`neutun -v` and `neutun --version` print:

```
neutun 1.0.6
```

Version sourced from `CARGO_PKG_VERSION`. `-v` is remapped from verbose to version. `--verbose` has no short alias.

## Migration

On first run, if `config.json` does not exist:
1. Check for old config files (`key.token`, `domain.txt`, `port.txt`).
2. For each file that exists, read its value. Missing files use the default value for that field.
3. Write `config.json` with migrated + default values.
4. Old files are left in place (not deleted) for safety.
5. Print: "Migrated config from legacy files to config.json"
6. If no old files exist either, the user is treated as not onboarded (triggers onboarding).

## Testing Plan

1. Compile with zero warnings.
2. Test against localhost as domain:
   - `neutun config onboard` with localhost settings
   - `neutun -p <port>` to start tunnel
   - `neutun saves add test` / `neutun saves restore test`
   - `neutun -p <port> --daemon` / `neutun daemon ls` / `neutun daemon stop`
   - `neutun -v` shows correct version
   - Interactive mode (bare `neutun`)
3. Push to GitHub and tag for binary build workflow.
