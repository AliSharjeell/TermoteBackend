![Termote Cover](public/maincover.png)

# Termote - Terminal Backend

<div align="center">

![Termote](https://img.shields.io/github/stars/AliSharjeell/Termote?style=social)

**Termote is a lightweight ADE (Agent Development Environment) that boosts your productivity with a persistent multi-pane workspace, built-in tools, and one-click remote access so you can keep working from your phone, anywhere.**

</div>

---

## Install

**This is the backend only.** For the full app with GUI, get the installer:

👉 **[Download from TermoteUI Releases](https://github.com/AliSharjeell/TermoteUI/releases)**

The installer packages everything together — one download, one install.

---

## What is This?

This is the **Termote backend** — a Rust application that:

- Manages PTY (pseudo-terminal) processes
- Serves terminal data over WebSocket
- Handles multi-client connections and state sync
- Integrates with Microsoft Dev Tunnels for remote access

It's bundled with the [TermoteUI](https://github.com/AliSharjeell/TermoteUI) desktop app. You don't need to install this directly — **download the installer from [TermoteUI Releases](https://github.com/AliSharjeell/TermoteUI/releases)**.

![Desktop Interface](public/mainss1.png)

---

## Mobile Access

Scan the QR code to connect from your phone or tablet — no VPN needed.

![Mobile QR Access](public/mobileqr.png)

---

![Mobile Interface](public/phone1new.png)

![Mobile Tabs View](public/phone2new.png)

---

## Technical Stack

| Component | Library |
|-----------|---------|
| HTTP/WebSocket | axum |
| Async runtime | tokio |
| PTY management | portable-pty |
| Logging | tracing + tracing-appender |
| Serialization | serde |

---

## Architecture

The backend runs as a sidecar alongside the frontend app:

```
┌──────────────────────────────────────┐
│         Termote Desktop App          │
│                                      │
│  ┌────────────┐     ┌─────────────┐  │
│  │  Frontend  │────▶│   Backend   │  │
│  │  (Next.js) │◀────│   (Rust)    │  │
│  └────────────┘ WS  └─────────────┘  │
│                        │             │
│                        ▼             │
│                   [PTY Processes]     │
└──────────────────────────────────────┘
```

**Ports:**
- `9090` — HTTP/WebSocket server
- `9091` — IPC for single-instance communication

---

## Building

```bash
# Build release
cargo build --release

# Binary output
# target/release/termote-backend
```

Or build via TermoteUI:
```bash
cd ../TermoteUI
npm run tauri:build
```

---

## IPC Commands

The backend accepts commands on port 9091:

| Command | Action |
|---------|--------|
| `open_dir:<path>` | Spawn new terminal in directory |
| `ban:<ip>` | Ban an IP address |
| `unban:<ip>` | Remove IP from ban list |
| `ban-list` | List all banned IPs |

---

## Logging

Logs are written to:
- `%TEMP%/termote.log` (rotated daily)
- stdout in debug builds

---

## Contributing

1. Open an Issue or email `alisharjeelofficial@gmail.com`
2. Get assigned before writing code
3. Open a PR with tests

---

## License

MIT License