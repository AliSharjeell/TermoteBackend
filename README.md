
<div align="center">

```
███████╗███████╗██████╗ ███╗   ███╗██████╗ ████████╗███████╗
╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔═══██╗╚══██╔══╝██╔════╝
 ██║   █████╗  ██████╔╝██╔████╔██║██║   ██║   ██║   █████╗
 ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║   ██║   ██║   ██╔══╝
   ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╔╝   ██║   ███████╗
   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝    ╚═╝   ╚══════╝
```
**Your local CLI, anywhere.**

Turn any browser into a full-powered, multi-pane terminal for your PC — instantly. No SSH, no tmux, no setup.

**Install the desktop app:** use the Tauri installer from [TermoteUI](https://github.com/AliSharjeell/TermoteUI). This backend repo is bundled into that desktop GUI as a sidecar.

</div>

---
Termote wraps your terminal sessions in encrypted WebSockets over HTTPS, punching through NATs and firewalls so you can access your machine's command line from any device with a browser. The supported end-user install is the Tauri desktop GUI from TermoteUI, which runs this Rust backend locally and forwards the same local GUI through Microsoft Dev Tunnels for mobile access.

## Core Features

| Feature | Description |
|---------|-------------|
| **Anywhere, Any Network** | Ditch the VPNs and port forwarding. Termote securely punches through NATs and firewalls, giving you instant access to your machine whether you are on the same Wi-Fi or halfway across the world. |
| **Multi-Pane Terminal (Like tmux, but visual)** | Don't limit yourself to one screen. Split, stack, and manage multiple terminal panes simultaneously right in your browser. Run your backend, watch your frontend build, and monitor server logs all in one view. |
| **Smart Single-Instance** | Already have Termote running? Typing `termote` in a new local folder or clicking "Open with Termote" won't spawn a redundant server. It intelligently connects to your active session and opens a new pane for that directory. |
| **Desktop GUI + Mobile Browser** | Install the Tauri GUI on your PC, then use the Mobile Access button to forward the same local GUI to your phone through Dev Tunnels. |
| **Runs directly on your PC (no cloud, no lag)** | You get the full unchained power of your host machine's CLI. Whatever your host PC can do, you can do remotely with near-zero latency. |
| **Drag-and-Drop File Transfer** | Drag files directly into any terminal pane to upload them to that pane's working directory. |
| **Security & Device Management** | View connected devices, kick sessions, and ban IP addresses directly from the UI. |
| **Pane Groups** | Color-code and organize terminal panes into groups for easier management. |

---

## Real-World Use Cases

### The Mobile AI Agent Commander
You are out grabbing coffee, but you want your beefy home rig to start working on a task. Pull out your phone, open Termote, and spin up AutoGPT, a local LLM, or a CLI-based AI agent. You can monitor its thought process and give it real-time corrections right from your mobile browser.

### The "Dinner Emergency" Server Fix
You are out with friends and get an alert that your local dev server, home lab, or Discord bot just crashed. Instead of rushing home, you quietly open Termote on your phone, run a quick `docker restart` or `pm2 reload`, and go right back to eating.

### Monitoring Heavy-Duty Jobs from the Couch
You just kicked off a massive compilation, a 4-hour web scraping script, or a machine learning training epoch on your desktop PC. Instead of sitting at your desk staring at a progress bar, grab your iPad, head to the couch, and keep a live Termote pane open next to your Netflix stream.

### Bypassing Locked-Down Networks
You are on a restrictive school or corporate Wi-Fi network that aggressively blocks SSH (Port 22). Because Termote wraps your terminal stream in standard, encrypted WebSockets over HTTPS (Port 443), it slices right through aggressive firewalls, letting you reach your home machine undetected.

---

## 60-Second Setup

1. Install or build the TermoteUI Tauri desktop app
2. Launch Termote on your PC
3. Click Mobile Access
4. Scan the QR code or copy the link for your phone

## Installation

### Recommended: Desktop App

Termote is no longer CLI-first. Install the desktop GUI from the TermoteUI releases page, or build it from source with both repos side by side:

```powershell
mkdir C:\AppsNew\TermoteFull
cd C:\AppsNew\TermoteFull
git clone https://github.com/AliSharjeell/Termote.git
git clone https://github.com/AliSharjeell/TermoteUI.git
cd TermoteUI
npm install
npm run tauri:build
```

The Tauri build exports the GUI, builds/prepares this Rust backend as a sidecar, and writes installer artifacts under:

```text
TermoteUI\src-tauri\target\release\bundle
```

Run the installed app. It starts this backend on `127.0.0.1:9090` and serves the bundled GUI from the same port. The Mobile Access button starts Microsoft Dev Tunnels for that port, so your phone opens the same local app through the tunnel.

### Legacy Backend-Only Install

This PowerShell installer is kept for backend/CLI development and compatibility. New users should prefer the desktop installer above.

```powershell
powershell -c "irm https://raw.githubusercontent.com/AliSharjeell/Termote/master/install.ps1 | iex"
```
> **Note:** The desktop app can use anonymous Dev Tunnels from the GUI. The legacy CLI installer may still ask you to authenticate with Microsoft Dev Tunnels.

**Troubleshooting:** If the `termote` command is not found immediately after installation, restart your terminal or run this to refresh your environment variables:

```powershell
$env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
```

## Connecting Devices (QR Code & Sharing)

Termote makes it easy to jump from your PC to your phone or another laptop right from the desktop GUI.

When you open your Termote dashboard, look for the connection tools built directly into the UI:

* **Mobile Access:** Click the Mobile Access button to start a Dev Tunnel for the local GUI/backend port.
* **QR Code for Mobile:** Open the profile panel and scan the QR code with your phone's camera.
* **Quick Share Link:** Copy the generated mobile URL and password from the GUI.

## Legacy CLI Commands

After the backend-only installer, the following commands are available in your terminal:

| Command | Description |
|---------|-------------|
| `termote` | Starts the server. If already running, opens the current directory as a new pane in your active session. |
| `termote-kill` | Safely shuts down all Termote instances and active tunnels. |
| `termote-link` | Displays your active tunnel URL, password, and shareable link. |
| `termote-log` | View real-time backend logs in your terminal. |
| `termote-ban-list` | View all banned IP addresses. |
| `termote-unban <ip>` | Remove an IP from the ban list. |
| `termote-update` | Fetch and execute the latest Termote installation script from GitHub. |
| Right-click in folder → **"Open with Termote"** | Instantly opens a new terminal pane for that specific folder in your existing web UI. |

---

## Why not SSH?

- No port forwarding
- No VPN setup
- Works on restricted networks (port 443)
- Browser-based UI (mobile friendly)
- Multi-pane built-in (no tmux needed)

## Security

- End-to-end encrypted via HTTPS/WebSockets
- Termote auth token required for every browser session
- Dev Tunnels can be anonymous; the Termote token remains the app-level gate
- No command logging or data storage
- Runs locally on your machine

## Reporting Issues

If you spot a bug, have a feature request, or the tunnel suddenly stops working, please open a new **Issue** in the repository. 

When creating an issue, please include as much detail as possible (your OS, error logs, and steps to reproduce) so it can be tracked and fixed quickly.

---

## Contributing

Contributions are highly encouraged! To keep development organized and prevent multiple people from doing the exact same work, please follow this strict workflow:

1. **Reach Out First:** If you want to build a feature or fix a bug, either comment on the specific Issue or email me directly at **alisharjeelofficial@gmail.com**.
2. **Get Assigned:** Wait for me to officially assign the task to you. 
3. **Open a Pull Request:** Once your code is ready and tested, open a standard PR on GitHub.
4. **Send a Demo Video:** To speed up the review process, you **must** email me a short screen recording demonstrating your changes working on your local machine. PRs without a demo video will not be merged.

For major architectural changes, please make sure we have thoroughly discussed your approach via email or in the issue thread before you start writing code!

## ⭐ Show Your Support

If Termote saved you from rushing home to fix a broken server, or made your mobile command-line life easier — give it a star!

[![Star](https://img.shields.io/github/stars/AliSharjeell/Termote?style=social)](https://github.com/AliSharjeell/Termote)

---

## License

This project is licensed under the MIT License.
