# Termote

**Your local CLI, anywhere.**

A zero-setup, multi-pane remote terminal that turns any web browser into a powerful command center for your PC.

```
███████╗███████╗██████╗ ███╗   ███╗██████╗ ████████╗███████╗
╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔═══██╗╚══██╔══╝██╔════╝
   ██║   █████╗  ██████╔╝██╔████╔██║██║   ██║   ██║   █████╗
   ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║   ██║   ██║   ██╔══╝
   ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╔╝   ██║   ███████╗
   ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝    ╚═╝   ╚══════╝
```

**Browser-accessible terminal multiplexer for Windows**

Termote wraps your terminal sessions in encrypted WebSockets over HTTPS, punching through NATs and firewalls so you can access your Windows machine's command line from any device with a browser.

---

## Core Features

| Feature | Description |
|---------|-------------|
| **Anywhere, Any Network** | Ditch the VPNs and port forwarding. Termote securely punches through NATs and firewalls, giving you instant access to your machine whether you are on the same Wi-Fi or halfway across the world. |
| **Infinite Multiplexing** | Don't limit yourself to one screen. Split, stack, and manage multiple terminal panes simultaneously right in your browser. Run your backend, watch your frontend build, and monitor server logs all in one view. |
| **Zero-Install Browser GUI** | Forget downloading bulky SSH clients on your phone or tablet. If a device has a web browser, it is now a fully functional command center with a beautiful, responsive UI. |
| **Native Local Execution** | You get the full unchained power of your host machine's CLI. Whatever your host PC can do, you can do remotely with near-zero latency. |

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

## Installation

### Quick Install (PowerShell)

Run this one-liner on your Windows machine:

```powershell
powershell -c "irm https://raw.githubusercontent.com/AliSharjeell/Termote/master/install.ps1 | iex"
```
Note: On your very first run, a browser window will pop up asking you to authenticate with GitHub or Microsoft. This is a one-time setup required by Microsoft Dev Tunnels to securely route your connection.

### Available Commands

After installation, the following commands are available in your terminal:

| Command | Description |
|---------|-------------|
| `termote` | Start or connect to Termote |
| `termote-kill` | Stop all Termote instances |
| `termote-link` | Show tunnel URL, password & share link |
| Right-click in folder → **"Open with Termote"** | Open a terminal pane at the current directory |

---

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
