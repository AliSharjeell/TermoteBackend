
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

**Access the Web Client:** [termote.vercel.app](https://termote.vercel.app)

</div>

---
Termote wraps your terminal sessions in encrypted WebSockets over HTTPS, punching through NATs and firewalls so you can access your Windows machine's command line from any device with a browser.

## Core Features

| Feature | Description |
|---------|-------------|
| **Anywhere, Any Network** | Ditch the VPNs and port forwarding. Termote securely punches through NATs and firewalls, giving you instant access to your machine whether you are on the same Wi-Fi or halfway across the world. |
| **Multi-Pane Terminal (Like tmux, but visual)** | Don't limit yourself to one screen. Split, stack, and manage multiple terminal panes simultaneously right in your browser. Run your backend, watch your frontend build, and monitor server logs all in one view. |
| **Smart Single-Instance** | Already have Termote running? Typing `termote` in a new local folder or clicking "Open with Termote" won't spawn a redundant server. It intelligently connects to your active session and opens a new pane for that directory. |
| **Zero-Install Browser GUI** | Forget downloading bulky SSH clients on your phone or tablet. Any device with a browser becomes your command center with a beautiful, responsive UI. |
| **Runs directly on your PC (no cloud, no lag)** | You get the full unchained power of your host machine's CLI. Whatever your host PC can do, you can do remotely with near-zero latency. |

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

1. Run the install command
2. Termote UI launches
3. Scan the QR code for your phone or copy the link for other devices
4. Control your PC terminal from anywhere

## Installation

### Quick Install (PowerShell)

Run this one-liner on your Windows machine:

```powershell
powershell -c "irm https://raw.githubusercontent.com/AliSharjeell/Termote/master/install.ps1 | iex"
```
> **Note:** On your very first run, a browser window will pop up asking you to authenticate with your Microsoft account. This is a one-time setup required by Microsoft Dev Tunnels to securely route your connection.

**Troubleshooting:** If the `termote` command is not found immediately after installation, restart your terminal or run this to refresh your environment variables:

```powershell
$env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
```

## Connecting Devices (QR Code & Sharing)

Termote makes it incredibly easy to jump from your PC to your phone or another laptop right from the web interface. 

When you open your Termote dashboard, look for the connection tools built directly into the UI:

* **QR Code for Mobile:** Click the QR code button in the web UI and scan it with your phone's camera to instantly open your terminal session on your mobile device.
* **Quick Share Link:** Use the shareable link button to copy your secure tunnel URL and password, ready to paste into any browser or send to a teammate.

## Available Commands

After installation, the following commands are available in your terminal:

| Command | Description |
|---------|-------------|
| `termote` | Starts the server. If already running, opens the current directory as a new pane in your active session. |
| `termote-kill` | Safely shuts down all Termote instances and active tunnels. |
| `termote-link` | Displays your active tunnel URL, password, and shareable link. |
| `termote-log` | View real-time backend logs in your terminal. |
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
- Auth required (Microsoft Dev Tunnels)
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
