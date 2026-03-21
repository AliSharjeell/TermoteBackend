# Termote

A web-native terminal multiplexer written in Rust. Run a lightweight daemon on your host machine and access, split, and manage terminal panes from any browser on any device.

## Features

- **Web-native**: Access your terminals from any modern browser
- **Split panes**: Create multiple terminal panes in a grid layout
- **Tabs view**: Organize terminals in a tabbed interface
- **Mobile ready**: Scan QR code to connect from your phone
- **Real-time**: WebSocket-powered instant response
- **Cross-device**: Seamless switching between desktop and mobile
- **Resize circuit breaker**: Prevents flicker when multiple devices connect

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                      Browser Clients                      │
│  (Desktop, Mobile, Tablet - any modern browser)          │
└─────────────────────────────────────────────────────────┘
                            │
                            │ WebSocket (wss://)
                            ▼
┌─────────────────────────────────────────────────────────┐
│                    Termote Backend                       │
│                                                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
│  │ WS Handler │  │  PTY Manager │  │ State (ARC) │    │
│  │  (axum)    │  │ (portable-pty)│  │  (RwLock)   │    │
│  └─────────────┘  └─────────────┘  └─────────────┘    │
│         │                                    │           │
│         │         Broadcast Channel          │           │
│         │      (tokio broadcast)            │           │
│         └──────────────┬────────────────────┘           │
│                        │                                 │
│  ┌────────────────────▼────────────────────────────┐   │
│  │              PTY Processes (spawned shells)       │   │
│  │   powershell.exe / cmd.exe / wsl                 │   │
│  └──────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────┘
```

### Components

- **axum WebSocket handler**: Handles client connections, authentication, and message routing
- **PTY Manager**: Spawns and manages pseudo-terminals using `portable-pty`
- **AppState**: Shared state across all WebSocket connections using `Arc<RwLock>`
- **Broadcast channel**: Radio tower pattern for multi-client sync

## Protocol

### Client → Server Messages

All messages are JSON with an `action` field:

```json
// Authenticate
{ "action": "auth", "token": "your-auth-token" }

// Spawn a new terminal
{ "action": "spawn", "shell": "powershell" }

// Send input to a pane
{ "action": "input", "pane_id": "uuid", "data": "ls -la\n" }

// Resize a pane (with circuit breaker)
{ "action": "resize", "pane_id": "uuid", "cols": 120, "rows": 30 }

// Force refocus (ignores circuit breaker, broadcasts to all)
{ "action": "refocus", "pane_id": "uuid", "cols": 120, "rows": 30 }

// Kill a pane
{ "action": "kill", "pane_id": "uuid" }

// Move pane to floating tabs
{ "action": "move_to_floating", "pane_id": "uuid" }

// Move pane to active grid
{ "action": "move_to_active", "pane_id": "uuid" }

// Rename a pane
{ "action": "rename", "pane_id": "uuid", "name": "My Server" }
```

### Server → Client Messages

All messages are JSON with an `event` field:

```json
// Authentication result
{ "event": "auth_result", "success": true, "message": "Authenticated" }

// State update (pane add/remove/resize)
{
  "event": "state_update",
  "panes": [{ "id": "uuid", "pid": 1234, "shell": "powershell", "name": "Shell 1", "cols": 80, "rows": 24 }],
  "active_panes": ["uuid"],
  "floating_panes": []
}

// Terminal output
{ "event": "output", "pane_id": "uuid", "data": "user@host:~$ " }
```

## Environment Variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `AUTH_TOKEN` | No | Auto-generated | Secret token for authentication |
| `PORT` | No | 9090 | HTTP server port |
| `FRONTEND_URL` | No | https://termux-web-frontend.vercel.app | URL of the frontend |
| `TUNNEL_URL` | No | ws://127.0.0.1:9090 | Public WebSocket URL of this server |

## Setup

### Prerequisites

- Rust 1.75+
- Windows (PTY support via `portable-pty`)

### Build

```bash
cargo build --release
```

### Run

```bash
# With defaults (auto-generates token, port 9090)
cargo run --release

# With environment variables
AUTH_TOKEN=my-secret-token PORT=8080 TUNNEL_URL=wss://example.com cargo run --release
```

### Example .env file

```env
AUTH_TOKEN=my-secret-token
PORT=9090
FRONTEND_URL=https://termote.example.com
TUNNEL_URL=wss://termote.example.com
```

## Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/ws` | GET | WebSocket connection |
| `/health` | GET | Health check |
| `/launch` | GET | Auto-login redirect to frontend |

## Security

- **Token authentication**: All WebSocket connections must authenticate within 5 seconds
- **CORS enabled**: Allows all origins for development
- **Auto-logout**: Connection closes if authentication fails

## Scrollback Buffer

Each pane maintains a 1MB scrollback buffer. When a new client connects, the buffer is replayed to provide context.

## Resize Circuit Breaker

To prevent resize conflicts when multiple devices connect:

1. **Frontend debouncing**: Resize requests are debounced 200ms and only sent if dimensions changed
2. **Backend circuit breaker**: Skips resize if dimensions match current state
3. **Refocus action**: Forces resize without circuit breaker for device switching

## Windows Support

Uses `portable-pty` for cross-platform PTY management. Supported shells:

- `powershell.exe` (default)
- `cmd.exe`
- `wsl` (Windows Subsystem for Linux)

## License

MIT
