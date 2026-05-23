# Termote - Lightweight Agent Development Environment

<div align="center">

```
███████╗███████╗██████╗ ███╗   ███╗██████╗ ████████╗███████╗
╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔═══██╗╚══██╔══╝██╔════╝
   ██║   █████╗  ██████╔╝██╔████╔██║██║   ██║   ██║   █████╗
   ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║   ██║   ██║   ██╔══╝
   ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╔╝   ██║   ███████╗
   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝    ╚═╝   ╚══════╝
```

**A lightweight ADE (Agent Development Environment) for power users.**

Turn any browser into a full-powered, multi-pane terminal workspace — with built-in tools, AI agent integration, and one-click remote access from anywhere.

</div>

---

## What is Termote?

**Termote** is a persistent multi-pane terminal workspace that supercharges your CLI workflow. Built with Rust for blazing performance, it wraps your terminal sessions in encrypted WebSockets over HTTPS, punching through NATs and firewalls so you can access your machine from any device, anywhere.

### Key Features

| Feature | Description |
|---------|-------------|
| **AI Agent Ready** | One-click launch for Claude Code, Gemini CLI, AutoGPT, and any CLI-based AI agent. Your agents run on your beefy home rig, accessible from anywhere. |
| **Multi-Pane Workspace** | Split, stack, and organize terminal panes like tmux — but with a beautiful visual UI. Color-code groups, drag-and-drop files, and manage everything from your phone. |
| **Anywhere Access** | Ditch the VPNs. Termote securely punches through NATs and firewalls via encrypted WebSockets over HTTPS (port 443). Work from your phone at the coffee shop, tablet on the couch, or laptop at the airport. |
| **Zero-Cloud Latency** | Runs locally on your machine. No cloud servers, no lag. Whatever your host PC can do, you can do remotely with near-zero latency. |
| **Smart Single-Instance** | Already running? New terminals intelligently connect to your active session and open in the right directory. No redundant servers. |
| **Security Built-In** | View connected devices, kick sessions, and ban IPs. End-to-end encrypted, token-gated access. |

---

## Quick Install

### One-Click Desktop Install (Recommended)

Download the Termote installer from the **[TermoteUI Releases Page](https://github.com/AliSharjeell/TermoteUI/releases)** and run it. The installer includes everything:

- **Termote desktop GUI** (Tauri + Next.js)
- **Bundled Rust backend** (PTy manager, WebSocket server, Dev Tunnel integration)
- **All prerequisites** (WebView2, Dev Tunnels CLI, etc.)
- **Start Menu shortcut** and optional context menu integration

Just download, run, and you're ready. One-click install, anywhere access.

### Build From Source

Clone both repos side by side:

```powershell
mkdir C:\AppsNew\TermoteFull
cd C:\AppsNew\TermoteFull
git clone https://github.com/AliSharjeell/Termote.git
git clone https://github.com/AliSharjeell/TermoteUI.git
cd TermoteUI
npm install
npm run tauri:build
```

Installer artifacts are in `src-tauri\target\release\bundle`.

---

## Architecture

**Termote** is a two-repo project:

| Repo | Role | Tech |
|------|------|------|
| **[Termote](https://github.com/AliSharjeell/Termote)** | Backend (this repo) | Rust, axum, portable-pty, tokio |
| **[TermoteUI](https://github.com/AliSharjeell/TermoteUI)** | Frontend & Desktop App | Next.js 16, React 19, xterm.js, Tauri 2 |

The TermoteUI desktop app bundles the Termote Rust backend as a sidecar. Users install one GUI app; the backend runs locally alongside it.

```
┌──────────────────────────────────────────────────────────┐
│                    Termote Desktop App                    │
│                    (TermoteUI + Tauri)                    │
│                                                          │
│  ┌────────────────────┐        ┌─────────────────────┐  │
│  │  WebView2 (Next.js)│◄──────►│  Termote Backend    │  │
│  │  xterm.js + React  │ WS     │  (Rust sidecar)     │  │
│  └────────────────────┘        └─────────────────────┘  │
└──────────────────────────────────────────────────────────┘
                           │
                           │ Dev Tunnel (HTTPS/443)
                           ▼
┌──────────────────────────────────────────────────────────┐
│                    Mobile Browser                        │
│              Same UI, anywhere in the world              │
└──────────────────────────────────────────────────────────┘
```

---

## Use Cases

### The Mobile AI Agent Commander
You're out grabbing coffee, but you want your beefy home rig to start training a model or running an AI agent. Pull out your phone, open Termote, and spin up Claude Code or a local LLM. Monitor its thought process, give real-time corrections, all from your mobile browser.

### The "Dinner Emergency" Fix
Get an alert that your dev server crashed? Instead of rushing home, open Termote on your phone, run `docker restart` or `pm2 reload`, and go right back to dinner.

### Monitoring Long Jobs from the Couch
Kicked off a massive compilation, a 4-hour scraping script, or ML training? Grab your tablet, head to the couch, and watch the progress in a live Termote pane next to your Netflix stream.

### Bypassing Restrictive Networks
On a corporate Wi-Fi that blocks SSH? Termote wraps your terminal in standard HTTPS WebSockets (port 443) and slices right through.

---

## Why Termote?

| SSH | Termote |
|-----|---------|
| Requires port forwarding | Works through NATs/firewalls |
| Needs VPN setup | Zero-config, just HTTPS |
| Terminal-only | Beautiful multi-pane UI |
| Not mobile-friendly | Optimized for phone/tablet |
| Manual session management | Persistent workspace, auto-reconnect |

---

## Technical Highlights

- **Rust PTY Management**: Native PTY handling via `portable-pty` for Windows ConPTY integration
- **Async WebSocket Server**: axum + tokio for high-performance concurrent connections
- **xterm.js Terminal Emulator**: Feature-rich terminal with FitAddon auto-sizing
- **Microsoft Dev Tunnels**: Anonymous tunnel access, QR code sharing, instant mobile connect
- **Multi-Client Sync**: Broadcast channel architecture for real-time multi-device sync
- **Scrollback Buffers**: Persistent terminal history across reconnections
- **Drag-and-Drop Upload**: File transfer directly into terminal working directory
- **Zustand State Management**: Lightweight, performant global state for React

---

## CLI Commands (Post-Install)

After installing, these commands are available:

| Command | Description |
|---------|-------------|
| `termote` | Start server (or open directory in active session) |
| `termote-kill` | Shutdown all Termote instances |
| `termote-link` | Display tunnel URL and shareable link |
| `termote-log` | Stream real-time backend logs |
| `termote-ban-list` | View banned IPs |
| `termote-unban <ip>` | Remove IP from ban list |
| Right-click folder → "Open with Termote" | Open new pane in that directory |

---

## Contributing

Contributions welcome! Please reach out before starting major work:

1. **Open an Issue** or email `alisharjeelofficial@gmail.com`
2. **Get assigned** before writing code
3. **Open a PR** with tests
4. **Send a demo video** to speed up review

---

## Show Your Support

If Termote made your mobile command-line life easier, give it a star!

[![Star](https://img.shields.io/github/stars/AliSharjeell/Termote?style=social)](https://github.com/AliSharjeell/Termote)

---

## License

MIT License