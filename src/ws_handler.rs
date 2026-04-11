//! WebSocket message handling for the terminal multiplexer.
//!
//! Handles client connections, authentication, and message routing.
//! Includes tunnel-check endpoint for Dev Tunnel session establishment.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State, ConnectInfo,
    },
    response::{IntoResponse, Response, Redirect},
    routing::get,
    Router,
};
use std::net::SocketAddr;
use futures_util::stream::StreamExt;
use futures_util::sink::SinkExt;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{info, error, warn};

use crate::messages::{ClientMessage, ServerMessage, PaneGroupInfo, DeviceInfo, DirectoryItem};
use crate::state::{AppState, ConnectedDevice, PaneGroup};

/// Decode base64 string to bytes.
fn base64_decode(input: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(input)
}

/// Maximum time to wait for authentication after connection.
const AUTH_TIMEOUT_SECS: u64 = 5;

/// WebSocket upgrade handler for /ws endpoint.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state, addr))
}

/// Handles a WebSocket connection from a client.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>, client_addr: SocketAddr) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::channel::<ServerMessage>(100);

    // Subscribe to broadcast channel (Radio Tower)
    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Spawn task to forward messages to WebSocket
    let sender_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            let json = match serde_json::to_string(&msg) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize message: {}", e);
                    continue;
                }
            };

            if sender.send(Message::Text(json)).await.is_err() {
                break;
            }
        }
    });

    // Use shared PTY manager from state (survives across reconnections)
    let tx_clone = tx.clone();
    let tx_for_broadcast = tx.clone();

    // Spawn task to forward broadcast messages to this client's channel
    let broadcast_task = tokio::spawn(async move {
        while let Ok(msg) = broadcast_rx.recv().await {
            if tx_for_broadcast.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Track this device's ID once authenticated
    let mut device_id: Option<String> = None;

    // Authentication state
    let mut authenticated = false;

    // Wait for authentication message with timeout
    let auth_result = timeout(
        Duration::from_secs(AUTH_TIMEOUT_SECS),
        receiver.next()
    ).await;

    match auth_result {
        Ok(Some(Ok(msg))) => {
            // Extract text from Message::Text variant
            let text = match msg {
                Message::Text(text) => text,
                Message::Close(_) => {
                    warn!("Client sent close message during auth");
                    let _ = tx.send(ServerMessage::AuthResult {
                        success: false,
                        message: Some("Unexpected close".to_string()),
                    }).await;
                    sender_task.abort();
                    broadcast_task.abort();
                    return;
                }
                _ => {
                    warn!("Received non-text message during auth");
                    let _ = tx.send(ServerMessage::AuthResult {
                        success: false,
                        message: Some("Expected text message".to_string()),
                    }).await;
                    sender_task.abort();
                    broadcast_task.abort();
                    return;
                }
            };

            match serde_json::from_str::<ClientMessage>(&text) {
                Ok(ClientMessage::Auth { token }) => {
                    if state.validate_token(&token).await {
                        authenticated = true;
                        info!("Client authenticated successfully");

                        // Add this device to the connected devices list
                        let ip = client_addr.ip().to_string();
                        let device = ConnectedDevice::new(ip, String::new());
                        device_id = Some(device.id.clone());
                        state.add_device(device).await;

                        // Broadcast updated device list to all connected clients
                        broadcast_device_list(&state).await;

                        let _ = tx.send(ServerMessage::AuthResult {
                            success: true,
                            message: Some("Authenticated".to_string()),
                        }).await;

                        // Send current state to newly authenticated client (via their own channel)
                        let panes = state.get_panes_info().await;
                        let active_panes = state.get_active_panes().await;
                        let floating_panes = state.get_floating_panes().await;
                        let groups = state.get_all_groups().await;
                        let _ = tx.send(ServerMessage::StateUpdate { panes, active_panes: active_panes.clone(), floating_panes: floating_panes.clone(), groups }).await;

                        // Replay scrollback buffers for all panes to this client
                        let all_pane_ids: Vec<String> = active_panes.iter().chain(floating_panes.iter()).cloned().collect();
                        for (pane_id, buffer) in state.get_panes_buffers(&all_pane_ids).await {
                            if !buffer.is_empty() {
                                let text = String::from_utf8_lossy(&buffer).to_string();
                                let _ = tx.send(ServerMessage::Output { pane_id, data: text }).await;
                            }
                        }
                    } else {
                        warn!("Invalid auth token attempted");
                        let _ = tx.send(ServerMessage::AuthResult {
                            success: false,
                            message: Some("Invalid token".to_string()),
                        }).await;
                        sender_task.abort();
                        broadcast_task.abort();
                        return;
                    }
                }
                Ok(_) => {
                    warn!("First message was not auth");
                    let _ = tx.send(ServerMessage::AuthResult {
                        success: false,
                        message: Some("Authentication required first".to_string()),
                    }).await;
                    sender_task.abort();
                    broadcast_task.abort();
                    return;
                }
                Err(e) => {
                    error!("Failed to parse auth message: {}", e);
                    let _ = tx.send(ServerMessage::AuthResult {
                        success: false,
                        message: Some("Invalid message format".to_string()),
                    }).await;
                    sender_task.abort();
                    return;
                }
            }
        }
        Ok(Some(Err(e))) => {
            error!("WebSocket error during auth: {}", e);
        }
        Ok(None) => {
            warn!("Client disconnected before auth");
        }
        Err(_) => {
            warn!("Auth timeout - no message received within {} seconds", AUTH_TIMEOUT_SECS);
            let _ = tx.send(ServerMessage::AuthResult {
                success: false,
                message: Some("Authentication timeout".to_string()),
            }).await;
        }
    }

    // Only proceed if authenticated
    if !authenticated {
        // Close connection after sending result
        sender_task.abort();
        return;
    }

    // Process messages
    while let Some(msg_result) = receiver.next().await {
        match msg_result {
            Ok(msg) => {
                match msg {
                    Message::Text(text) => {
                        match serde_json::from_str::<ClientMessage>(&text) {
                            Ok(client_msg) => {
                                if let Err(e) = handle_client_message(
                                    client_msg,
                                    &state,
                                    &tx_clone,
                                ).await {
                                    error!("Error handling client message: {}", e);
                                }
                            }
                            Err(e) => {
                                error!("Failed to parse client message: {}", e);
                            }
                        }
                    }
                    Message::Close(_) => {
                        info!("Client disconnected normally");
                        break;
                    }
                    _ => {
                        // Ignore other message types
                    }
                }
            }
            Err(e) => {
                error!("WebSocket error: {}", e);
                break;
            }
        }
    }

    // Cleanup - remove device from tracking and broadcast updated list
    info!("Cleaning up WebSocket session");
    if authenticated {
        if let Some(id) = device_id {
            state.remove_device(&id).await;
            broadcast_device_list(&state).await;
        }
    }
    sender_task.abort();
    broadcast_task.abort();
}

/// Helper to broadcast a state update with groups.
async fn broadcast_state_update(state: &AppState) {
    let panes = state.get_panes_info().await;
    let active_panes = state.get_active_panes().await;
    let floating_panes = state.get_floating_panes().await;
    let groups = state.get_all_groups().await;
    let _ = state.broadcast_tx.send(ServerMessage::StateUpdate {
        panes,
        active_panes,
        floating_panes,
        groups,
    });
}

/// Helper to broadcast the current device list to all clients.
async fn broadcast_device_list(state: &AppState) {
    let devices = state.get_connected_devices().await;
    let device_infos: Vec<DeviceInfo> = devices.iter().map(DeviceInfo::from).collect();
    let _ = state.broadcast_tx.send(ServerMessage::DeviceList { devices: device_infos });
}

/// Runs git status and returns parsed result.
async fn run_git_status(dir: &str, pane_id: &str) -> ServerMessage {
    use std::process::Command;

    // Check if it's a git repo
    let git_dir = std::path::Path::new(dir).join(".git");
    if !git_dir.exists() {
        return ServerMessage::GitStatus {
            pane_id: pane_id.to_string(),
            dir: dir.to_string(),
            is_repo: false,
            branch: None,
            staged: vec![],
            unstaged: vec![],
            untracked: vec![],
            ahead: None,
            behind: None,
        };
    }

    // Get branch name
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(dir)
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

    // Get status --porcelain for easy parsing
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(dir)
        .output()
        .ok();

    let (staged, unstaged, untracked) = if let Some(output) = status_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let mut s = Vec::new();
            let mut u = Vec::new();
            let mut n = Vec::new();

            for line in stdout.lines() {
                if line.len() < 3 {
                    continue;
                }
                let index = &line[..1];
                let worktree = &line[1..2];
                let filepath = line[3..].to_string();

                // Staged (index) changes
                if index != " " && index != "?" {
                    s.push(filepath.clone());
                }
                // Untracked files
                if index == "?" {
                    n.push(filepath.clone());
                }
                // Unstaged (worktree) changes
                if worktree != " " && worktree != "?" {
                    u.push(filepath.clone());
                }
            }
            (s, u, n)
        } else {
            (vec![], vec![], vec![])
        }
    } else {
        (vec![], vec![], vec![])
    };

    // Get ahead/behind vs remote
    let (ahead, behind) = if branch.is_some() {
        let rev_output = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
            .current_dir(dir)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

        if let Some(rev_str) = rev_output {
            let parts: Vec<&str> = rev_str.split_whitespace().collect();
            if parts.len() == 2 {
                let ahead_val = parts[0].parse::<i32>().ok();
                let behind_val = parts[1].parse::<i32>().ok();
                (ahead_val, behind_val)
            } else {
                (None, None)
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    ServerMessage::GitStatus {
        pane_id: pane_id.to_string(),
        dir: dir.to_string(),
        is_repo: true,
        branch,
        staged,
        unstaged,
        untracked,
        ahead,
        behind,
    }
}

/// Runs git commit for staged changes.
async fn run_git_commit(dir: &str, pane_id: &str, message: &str) -> ServerMessage {
    use std::process::Command;

    // First check if there are staged changes
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1", "-s"])
        .current_dir(dir)
        .output();

    let has_staged = if let Ok(output) = status_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.lines().any(|line| {
                if line.len() < 3 { return false; }
                let index = &line[..1];
                index != " " && index != "?"
            })
        } else {
            false
        }
    } else {
        false
    };

    if !has_staged {
        return ServerMessage::GitCommitResult {
            pane_id: pane_id.to_string(),
            success: false,
            message: "No staged changes to commit".to_string(),
        };
    }

    // Run git commit
    let commit_output = Command::new("git")
        .args(["commit", "-m", message])
        .current_dir(dir)
        .output();

    match commit_output {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                ServerMessage::GitCommitResult {
                    pane_id: pane_id.to_string(),
                    success: true,
                    message: stdout.to_string(),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                ServerMessage::GitCommitResult {
                    pane_id: pane_id.to_string(),
                    success: false,
                    message: format!("Commit failed: {}", stderr),
                }
            }
        }
        Err(e) => ServerMessage::GitCommitResult {
            pane_id: pane_id.to_string(),
            success: false,
            message: format!("Failed to execute git commit: {}", e),
        },
    }
}

/// Runs git add or git reset for specified files.
async fn run_git_stage(dir: &str, _pane_id: &str, files: &[String], unstage: bool) -> ServerMessage {
    use std::process::Command;

    if files.is_empty() {
        return ServerMessage::Error {
            message: "No files specified".to_string(),
        };
    }

    let result = if unstage {
        // git reset HEAD -- <files>
        let mut cmd = Command::new("git");
        cmd.args(["reset", "HEAD", "--"]).args(files);
        cmd.current_dir(dir);
        cmd.output()
    } else {
        // git add -- <files>
        let mut cmd = Command::new("git");
        cmd.args(["add", "--"]).args(files);
        cmd.current_dir(dir);
        cmd.output()
    };

    match result {
        Ok(output) => {
            if output.status.success() {
                let action = if unstage { "unstaged" } else { "staged" };
                ServerMessage::Error {
                    message: format!("{} {} file(s)", action, files.len()),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                ServerMessage::Error {
                    message: format!("Failed to {} files: {}", if unstage { "unstage" } else { "stage" }, stderr),
                }
            }
        }
        Err(e) => ServerMessage::Error {
            message: format!("Failed to execute git command: {}", e),
        },
    }
}

/// Runs git push.
async fn run_git_push(dir: &str, _pane_id: &str) -> ServerMessage {
    use std::process::Command;

    let output = Command::new("git")
        .args(["push"])
        .current_dir(dir)
        .output();

    match output {
        Ok(o) => {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                ServerMessage::Error {
                    message: format!("Pushed successfully: {}", stdout.trim()),
                }
            } else {
                let stderr = String::from_utf8_lossy(&o.stderr);
                ServerMessage::Error {
                    message: format!("Push failed: {}", stderr.trim()),
                }
            }
        }
        Err(e) => ServerMessage::Error {
            message: format!("Failed to execute git push: {}", e),
        },
    }
}

/// Runs git pull.
async fn run_git_pull(dir: &str, _pane_id: &str) -> ServerMessage {
    use std::process::Command;

    let output = Command::new("git")
        .args(["pull"])
        .current_dir(dir)
        .output();

    match output {
        Ok(o) => {
            if o.status.success() {
                let stdout = String::from_utf8_lossy(&o.stdout);
                ServerMessage::Error {
                    message: format!("Pulled successfully: {}", stdout.trim()),
                }
            } else {
                let stderr = String::from_utf8_lossy(&o.stderr);
                ServerMessage::Error {
                    message: format!("Pull failed: {}", stderr.trim()),
                }
            }
        }
        Err(e) => ServerMessage::Error {
            message: format!("Failed to execute git pull: {}", e),
        },
    }
}

/// Finds all git repositories in subdirectories of a path.
async fn run_get_port_processes() -> Vec<crate::messages::PortProcess> {
    use std::collections::HashMap;
    use std::process::Command;

    // First get a map of PID -> process name from tasklist
    let mut pid_to_name: HashMap<u32, String> = HashMap::new();
    if let Ok(task_output) = Command::new("tasklist")
        .args(["/FI", "STATUS eq Running", "/FO", "CSV", "/NH"])
        .output()
    {
        if task_output.status.success() {
            for line in String::from_utf8_lossy(&task_output.stdout).lines() {
                // CSV format: "name","pid","session","sessionnum","memory"
                if let Some(name) = line.strip_prefix('"') {
                    let parts: Vec<&str> = name.split('"').collect();
                    if parts.len() >= 2 {
                        let name = parts[0].trim();
                        if let Some(pid_str) = parts.get(1) {
                            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                                pid_to_name.insert(pid, name.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    let mut processes = vec![];
    let mut seen: std::collections::HashSet<u16> = std::collections::HashSet::new();

    // Use netstat to find listening ports with process IDs
    let output = Command::new("netstat")
        .args(["-ano", "-p", "TCP"])
        .output()
        .ok();

    let Some(output) = output else { return processes };
    if !output.status.success() { return processes }

    let stdout = String::from_utf8_lossy(&output.stdout);

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        // netstat -ano output: Proto Local Address Foreign Address State PID
        // Example: TCP 0.0.0.0:3000 0.0.0.0:0 LISTENING 12345
        if parts.len() >= 5 && parts[3] == "LISTENING" {
            let local_addr = parts[1];
            // Extract port from local address like "0.0.0.0:3000" or "[::]:3000"
            if let Some(colon_pos) = local_addr.rfind(':') {
                if let Ok(port) = local_addr[colon_pos + 1..].parse::<u16>() {
                    if let Ok(pid) = parts[4].parse::<u32>() {
                        // Skip system ports (1024 and below) to reduce noise
                        // Also deduplicate by port
                        if port > 1024 && seen.insert(port) {
                            let process_name = pid_to_name.get(&pid)
                                .cloned()
                                .unwrap_or_else(|| format!("PID:{}", pid));
                            processes.push(crate::messages::PortProcess {
                                port,
                                pid,
                                process_name,
                                cwd: None,
                            });
                        }
                    }
                }
            }
        }
    }

    processes.sort_by_key(|p| p.port);
    processes
}

async fn run_find_git_repos(path: &str) -> ServerMessage {
    use std::process::Command;
    use std::path::Path;

    let mut repos = vec![];

    // First check if the path itself is a git repo
    let git_dir = Path::new(path).join(".git");
    if git_dir.exists() {
        let branch = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(path)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

        let name = Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());

        repos.push(crate::messages::GitRepoInfo {
            path: path.to_string(),
            name,
            branch,
        });
    }

    // Walk through immediate subdirectories looking for .git folders
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                let sub_git = entry_path.join(".git");
                if sub_git.exists() {
                    let branch = Command::new("git")
                        .args(["rev-parse", "--abbrev-ref", "HEAD"])
                        .current_dir(&entry_path)
                        .output()
                        .ok()
                        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

                    let name = entry_path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| entry_path.to_string_lossy().to_string());

                    repos.push(crate::messages::GitRepoInfo {
                        path: entry_path.to_string_lossy().to_string(),
                        name,
                        branch,
                    });
                }
            }
        }
    }

    ServerMessage::GitReposFound { repos }
}

/// Runs git log to get commit history.
async fn run_git_log(dir: &str, pane_id: &str) -> ServerMessage {
    use std::process::Command;

    // git log --oneline -20 with format: %H|%s|%an|%ad
    let output = Command::new("git")
        .args(["log", "--oneline", "--format=%H|%s|%an|%ad", "-100"])
        .current_dir(dir)
        .output();

    let commits = match output {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout.lines()
                .filter(|line| !line.is_empty())
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(4, '|').collect();
                    let hash = parts.get(0).unwrap_or(&"").to_string();
                    crate::messages::GitCommitInfo {
                        hash: hash.clone(),
                        short_hash: hash.chars().take(7).collect(),
                        message: parts.get(1).unwrap_or(&"").to_string(),
                        author: parts.get(2).unwrap_or(&"").to_string(),
                        date: parts.get(3).unwrap_or(&"").to_string(),
                    }
                })
                .collect()
        }
        _ => vec![],
    };

    ServerMessage::GitLog {
        pane_id: pane_id.to_string(),
        dir: dir.to_string(),
        commits,
    }
}

/// Runs full source control state: git status + ahead/behind + outgoing commits.
async fn run_source_control_state(path: &str) -> ServerMessage {
    use std::process::Command;

    let git_dir = std::path::Path::new(path).join(".git");
    if !git_dir.exists() {
        return ServerMessage::SourceControlState {
            path: path.to_string(),
            is_repo: false,
            branch: None,
            remote: None,
            staged: vec![],
            unstaged: vec![],
            untracked: vec![],
            ahead: 0,
            behind: 0,
            outgoing_commits: vec![],
        };
    }

    // Get branch and remote
    let branch = Command::new("git")
        .args(["branch", "--show-current"])
        .current_dir(path)
        .output()
        .ok()
        .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

    let remote = if let Some(ref br) = branch {
        // Get the remote for this branch
        let remote_output = Command::new("git")
            .args(["config", &format!("branch.{}.remote", br)])
            .current_dir(path)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });
        remote_output
    } else {
        None
    };

    // Get status --porcelain
    let status_output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(path)
        .output();

    let mut staged = vec![];
    let mut unstaged = vec![];
    let mut untracked = vec![];

    if let Ok(output) = status_output {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                if line.len() < 3 { continue; }
                let index = &line[..1];
                let worktree = &line[1..2];
                let filepath = line[3..].to_string();
                let status = line[..2].to_string();

                if index == "?" {
                    untracked.push(crate::messages::SourceControlFile { path: filepath.clone(), status: "untracked".to_string(), added: None, deleted: None });
                } else if index != " " {
                    staged.push(crate::messages::SourceControlFile { path: filepath.clone(), status: status.clone(), added: None, deleted: None });
                }
                if worktree != " " && worktree != "?" {
                    unstaged.push(crate::messages::SourceControlFile { path: filepath.clone(), status, added: None, deleted: None });
                }
            }
        }
    }

    // Get ahead/behind
    let (ahead, behind) = if remote.is_some() && branch.is_some() {
        let rev_output = Command::new("git")
            .args(["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])
            .current_dir(path)
            .output()
            .ok()
            .and_then(|o| if o.status.success() { Some(String::from_utf8_lossy(&o.stdout).trim().to_string()) } else { None });

        if let Some(rev_str) = rev_output {
            let parts: Vec<&str> = rev_str.split_whitespace().collect();
            if parts.len() == 2 {
                let a = parts[0].parse::<i32>().unwrap_or(0);
                let b = parts[1].parse::<i32>().unwrap_or(0);
                (a, b)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        }
    } else {
        (0, 0)
    };

    // Get outgoing commits (not on remote)
    let mut outgoing_commits = vec![];
    if let (Some(br), Some(ref rem)) = (&branch, &remote) {
        let upstream = format!("{}/{}", rem, br);
        let log_output = Command::new("git")
            .args(["log", "--oneline", &format!("{}..HEAD", upstream), "-15"])
            .current_dir(path)
            .output();

        if let Ok(output) = log_output {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if line.is_empty() { continue; }
                    let parts: Vec<&str> = line.splitn(2, ' ').collect();
                    let hash = parts.get(0).unwrap_or(&"").to_string();
                    let message = parts.get(1).unwrap_or(&"").to_string();
                    outgoing_commits.push(crate::messages::OutgoingCommit {
                        hash: hash.clone(),
                        short_hash: hash.chars().take(7).collect(),
                        message,
                        author: "".to_string(),
                        date: "".to_string(),
                    });
                }
            }
        }
    }

    ServerMessage::SourceControlState {
        path: path.to_string(),
        is_repo: true,
        branch,
        remote,
        staged,
        unstaged,
        untracked,
        ahead,
        behind,
        outgoing_commits,
    }
}

/// Handles a parsed client message.
async fn handle_client_message(
    msg: ClientMessage,
    state: &AppState,
    _tx: &mpsc::Sender<ServerMessage>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match msg {
        ClientMessage::Spawn { shell } => {
            info!("Spawn request for shell: {}", shell);

            let (pane_id, pid) = state.pty_manager.spawn_pty(
                &shell,
                80,
                24,
                state.clone(),
                &state.broadcast_tx,
            )?;

            // Update pane with actual PID
            let mut pane = crate::state::Pane::new(pid, shell, 80, 24);
            pane.id = pane_id.clone();
            state.add_pane(pane).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;

            info!("Spawned pane {} with PID {}", pane_id, pid);
        }

        ClientMessage::SpawnAtDir { shell, dir } => {
            info!("Spawn at directory request: {} in {}", shell, dir);

            if let Err(e) = state.spawn_pane_at_dir(&dir, &shell).await {
                error!("Failed to spawn pane at {}: {}", dir, e);
                let _ = state.broadcast_tx.send(ServerMessage::Error {
                    message: format!("Failed to spawn terminal at {}: {}", dir, e),
                });
            }
            // spawn_pane_at_dir already broadcasts state_update
        }

        ClientMessage::SpawnBrowser { url } => {
            info!("Spawn browser request: {}", url);

            #[cfg(windows)]
            {
                use std::process::Command;
                // Open URL in default browser using cmd /c start
                let result = Command::new("cmd")
                    .args(["/c", "start", "", &url])
                    .spawn();

                match result {
                    Ok(child) => {
                        info!("Browser opened with PID: {:?}", child.id());
                        let _ = state.broadcast_tx.send(ServerMessage::Error {
                            message: format!("Opened {} in browser", url),
                        });
                    }
                    Err(e) => {
                        error!("Failed to open browser: {}", e);
                        let _ = state.broadcast_tx.send(ServerMessage::Error {
                            message: format!("Failed to open browser: {}", e),
                        });
                    }
                }
            }
        }

        ClientMessage::RequestDirectoryPicker { shell } => {
            info!("Directory picker requested for shell: {}", shell);

            // Open native folder picker on a blocking thread (rfd requires main thread on Windows)
            let dir = tokio::task::spawn_blocking(move || {
                let picker = rfd::FileDialog::new()
                    .set_title("Select Terminal Directory")
                    .pick_folder();

                picker.map(|p| p.to_string_lossy().to_string())
            }).await;

            match dir {
                Ok(Some(selected_dir)) => {
                    info!("Directory selected: {}", selected_dir);
                    if let Err(e) = state.spawn_pane_at_dir(&selected_dir, &shell).await {
                        error!("Failed to spawn pane at {}: {}", selected_dir, e);
                    }
                    // spawn_pane_at_dir already broadcasts state_update
                }
                Ok(None) => {
                    info!("Directory picker cancelled");
                    // User cancelled - notify client
                    let _ = state.broadcast_tx.send(ServerMessage::DirectoryPickerCancelled);
                }
                Err(e) => {
                    error!("Directory picker error: {}", e);
                    let _ = state.broadcast_tx.send(ServerMessage::DirectoryPickerCancelled);
                }
            }
        }

        ClientMessage::ListDirectory { path } => {
            info!("List directory requested: {:?}", path);

            // Detect empty root: empty string, null, or exactly "/"
            let is_empty_root = match &path {
                Some(p) => p.is_empty() || p == "/",
                None => true,
            };

            let target_path = if is_empty_root {
                None
            } else {
                path.as_deref()
            };

            let (result_path, items) = match target_path {
                Some(p) => {
                    // List the specified directory
                    let path_str = p.to_string();
                    let dir_items = match std::fs::read_dir(p) {
                        Ok(entries) => {
                            let mut items: Vec<DirectoryItem> = Vec::new();
                            for entry in entries.flatten() {
                                let entry_path = entry.path();
                                let name = entry.file_name().to_string_lossy().to_string();
                                // Skip hidden files/folders on Windows
                                if name.starts_with('.') {
                                    continue;
                                }
                                let is_dir = entry_path.is_dir();
                                let absolute_path = entry_path.to_string_lossy().to_string();
                                items.push(DirectoryItem {
                                    name,
                                    absolute_path,
                                    is_dir,
                                });
                            }
                            // Sort: directories first, then alphabetically
                            items.sort_by(|a, b| {
                                match (a.is_dir, b.is_dir) {
                                    (true, false) => std::cmp::Ordering::Less,
                                    (false, true) => std::cmp::Ordering::Greater,
                                    _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                                }
                            });
                            Ok(items)
                        }
                        Err(e) => {
                            warn!("Failed to read directory {}: {}", p, e);
                            Ok(Vec::new()) // Return empty on permission errors
                        }
                    };
                    (path_str, dir_items)
                }
                None => {
                    // Return available drives on Windows
                    let mut items = Vec::new();
                    #[cfg(windows)]
                    {
                        // Check common Windows drive letters
                        for letter in b'A'..=b'Z' {
                            let drive = format!("{}:\\", letter as char);
                            let path = std::path::Path::new(&drive);
                            if path.exists() {
                                items.push(DirectoryItem {
                                    name: format!("{}:\\", letter as char),
                                    absolute_path: drive,
                                    is_dir: true,
                                });
                            }
                        }
                    }
                    #[cfg(not(windows))]
                    {
                        // On Unix, return root
                        items.push(DirectoryItem {
                            name: "/".to_string(),
                            absolute_path: "/".to_string(),
                            is_dir: true,
                        });
                    }
                    let home_dir = dirs::home_dir();
                    let result_path = home_dir
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| "/".to_string());
                    (result_path, Ok(items))
                }
            };

            let response_items = items.unwrap_or_else(|_: std::io::Error| Vec::new());
            let _ = state.broadcast_tx.send(ServerMessage::DirectoryContents {
                path: result_path,
                items: response_items,
            });
        }

        ClientMessage::Input { pane_id, data } => {
            info!("Backend received input for pane: {}", pane_id);
            if let Err(e) = state.pty_manager.write_input(&pane_id, &data) {
                error!("Failed to write input to pane {}: {}", pane_id, e);
            }
        }

        ClientMessage::Resize { pane_id, cols, rows } => {
            // Circuit breaker: only resize if dimensions actually changed
            let should_resize = if let Some(current_pane) = state.get_pane(&pane_id).await {
                current_pane.cols != cols || current_pane.rows != rows
            } else {
                true
            };

            if !should_resize {
                info!("Resize skipped for pane {} (same dimensions {}x{})", pane_id, cols, rows);
            } else {
                info!("Resize pane {} to {}x{}", pane_id, cols, rows);
                state.resize_pane(&pane_id, cols, rows).await;
                if let Err(e) = state.pty_manager.resize_pty(&pane_id, cols, rows) {
                    error!("Failed to resize pane {}: {}", pane_id, e);
                }

                // Broadcast state update to all clients
                broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
            }
        }

        ClientMessage::Kill { pane_id } => {
            info!("Kill request for pane {}", pane_id);

            // Kill the PTY
            if let Err(e) = state.pty_manager.kill_pty(&pane_id) {
                warn!("Error killing PTY: {}", e);
            }

            // Remove from state
            state.remove_pane(&pane_id).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::MoveToFloating { pane_id } => {
            info!("Move to floating: {}", pane_id);
            state.move_to_floating(&pane_id).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::MoveToActive { pane_id } => {
            info!("Move to active: {}", pane_id);
            state.move_to_active(&pane_id).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::Rename { pane_id, name } => {
            info!("Rename pane {} to {}", pane_id, name);
            state.rename_pane(&pane_id, &name).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::Refocus { pane_id, cols, rows } => {
            // Force resize without circuit breaker - this client takes priority
            info!("Refocus pane {} to {}x{} (forced)", pane_id, cols, rows);
            state.resize_pane(&pane_id, cols, rows).await;
            if let Err(e) = state.pty_manager.resize_pty(&pane_id, cols, rows) {
                error!("Failed to resize pane {}: {}", pane_id, e);
            }

            // Broadcast to ALL clients (including sender) so everyone updates
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::Auth { .. } => {
            // Already handled - shouldn't get here
            warn!("Auth message received after authentication");
        }

        ClientMessage::Ping => {
            // No-op: heartbeat to keep connection alive
            tracing::debug!("Received ping from client");
        }

        ClientMessage::CreateGroup { id, name, color } => {
            info!("Create group: {} ({})", name, color);
            let group = if let Some(id) = id {
                PaneGroup { id, name: name.clone(), color: color.clone() }
            } else {
                PaneGroup::new(name.clone(), color.clone())
            };
            state.add_group(group.clone()).await;

            // Broadcast group created event
            let group_info = PaneGroupInfo {
                id: group.id.clone(),
                name: group.name.clone(),
                color: group.color.clone(),
            };
            let _ = state.broadcast_tx.send(ServerMessage::GroupCreated { group: group_info });

            // Broadcast state update to sync groups
            broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
        }

        ClientMessage::DeleteGroup { group_id } => {
            info!("Delete group: {}", group_id);
            if state.remove_group(&group_id).await {
                // Broadcast group deleted event
                let _ = state.broadcast_tx.send(ServerMessage::GroupDeleted { group_id: group_id.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
            }
        }

        ClientMessage::RenameGroup { group_id, name } => {
            info!("Rename group {} to {}", group_id, name);
            if state.rename_group(&group_id, &name).await {
                // Broadcast group renamed event
                let _ = state.broadcast_tx.send(ServerMessage::GroupRenamed { group_id: group_id.clone(), name: name.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
            }
        }

        ClientMessage::SetPaneGroup { pane_id, group_id } => {
            info!("Set pane {} group to {:?}", pane_id, group_id);
            if state.set_pane_group(&pane_id, group_id.as_deref()).await {
                // Broadcast pane group set event
                let _ = state.broadcast_tx.send(ServerMessage::PaneGroupSet { pane_id: pane_id.clone(), group_id: group_id.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            // Persist session state
            state.save_session().await;
            }
        }

        ClientMessage::GetDeviceList => {
            info!("Device list requested");
            let devices = state.get_connected_devices().await;
            let device_infos: Vec<DeviceInfo> = devices.iter().map(DeviceInfo::from).collect();
            let _ = state.broadcast_tx.send(ServerMessage::DeviceList { devices: device_infos });
        }

        ClientMessage::GetPortProcesses => {
            info!("Port processes requested");
            let processes = run_get_port_processes().await;
            let _ = state.broadcast_tx.send(ServerMessage::PortProcesses { processes });
        }

        ClientMessage::KillProcess { pid } => {
            info!("Kill process requested: PID {}", pid);
            let output = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .output();
            match output {
                Ok(out) if out.status.success() => {
                    let _ = state.broadcast_tx.send(ServerMessage::ProcessKilled {
                        pid,
                        success: true,
                        message: "Process killed".to_string(),
                    });
                }
                Ok(_) => {
                    let _ = state.broadcast_tx.send(ServerMessage::ProcessKilled {
                        pid,
                        success: false,
                        message: "Failed to kill process".to_string(),
                    });
                }
                Err(e) => {
                    let _ = state.broadcast_tx.send(ServerMessage::ProcessKilled {
                        pid,
                        success: false,
                        message: e.to_string(),
                    });
                }
            }
        }

        ClientMessage::KickDevice { device_id } => {
            info!("Kick device requested: {}", device_id);
            // Check if device exists before removing
            let devices = state.get_connected_devices().await;
            if devices.iter().any(|d| d.id == device_id) {
                state.remove_device(&device_id).await;
                let _ = state.broadcast_tx.send(ServerMessage::DeviceKicked { device_id: device_id.clone() });
            } else {
                let _ = state.broadcast_tx.send(ServerMessage::Error { message: "Device not found".to_string() });
            }
        }

        ClientMessage::BanDevice { ip } => {
            info!("Ban device requested for IP: {}", ip);
            // Check if already banned
            if state.is_ip_banned(&ip).await {
                let _ = state.broadcast_tx.send(ServerMessage::Error { message: "IP already banned".to_string() });
            } else {
                state.ban_ip(&ip).await;
                // Kick any devices with this IP
                let devices = state.get_connected_devices().await;
                for device in devices {
                    if device.ip == ip {
                        let _ = state.broadcast_tx.send(ServerMessage::DeviceKicked { device_id: device.id.clone() });
                    }
                }
                let _ = state.broadcast_tx.send(ServerMessage::DeviceBanned { ip: ip.clone() });
            }
        }

        ClientMessage::GetGitStatus { pane_id } => {
            info!("Git status requested for pane: {}", pane_id);

            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            match cwd {
                Some(dir) => {
                    let result = run_git_status(&dir, &pane_id).await;
                    let _ = state.broadcast_tx.send(result);
                }
                None => {
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: "Pane has no working directory".to_string(),
                    });
                }
            }
        }

        ClientMessage::GitCommit { pane_id, message } => {
            info!("Git commit requested for pane: {}", pane_id);

            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            match cwd {
                Some(dir) => {
                    let result = run_git_commit(&dir, &pane_id, &message).await;
                    let _ = state.broadcast_tx.send(result);
                }
                None => {
                    let _ = state.broadcast_tx.send(ServerMessage::GitCommitResult {
                        pane_id: pane_id.clone(),
                        success: false,
                        message: "Pane has no working directory".to_string(),
                    });
                }
            }
        }

        ClientMessage::GitStage { pane_id, files, unstage } => {
            info!("Git stage/unstage requested for pane: {}", pane_id);

            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            match cwd {
                Some(dir) => {
                    let result = run_git_stage(&dir, &pane_id, &files, unstage).await;
                    let _ = state.broadcast_tx.send(result);
                    // Refresh status after stage/unstage
                    let status_result = run_git_status(&dir, &pane_id).await;
                    let _ = state.broadcast_tx.send(status_result);
                }
                None => {
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: "Pane has no working directory".to_string(),
                    });
                }
            }
        }

        ClientMessage::GitLog { pane_id, dir } => {
            info!("Git log requested for pane: {} dir: {}", pane_id, dir);
            if dir.is_empty() {
                let _ = state.broadcast_tx.send(ServerMessage::Error {
                    message: "No directory provided for git log".to_string(),
                });
            } else {
                let result = run_git_log(&dir, &pane_id).await;
                let _ = state.broadcast_tx.send(result);
            }
        }

        ClientMessage::GetSourceControlState { path } => {
            info!("Source control state requested for: {}", path);
            let result = run_source_control_state(&path).await;
            let _ = state.broadcast_tx.send(result);
        }

        ClientMessage::FindGitRepos { path } => {
            info!("Finding git repos in: {}", path);
            let result = run_find_git_repos(&path).await;
            let _ = state.broadcast_tx.send(result);
        }

        ClientMessage::GitPush { pane_id } => {
            info!("Git push requested for pane: {}", pane_id);

            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            match cwd {
                Some(dir) => {
                    let result = run_git_push(&dir, &pane_id).await;
                    let _ = state.broadcast_tx.send(result);
                    // Refresh state after push
                    let state_result = run_source_control_state(&dir).await;
                    let _ = state.broadcast_tx.send(state_result);
                }
                None => {
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: "Pane has no working directory".to_string(),
                    });
                }
            }
        }

        ClientMessage::GitPull { pane_id } => {
            info!("Git pull requested for pane: {}", pane_id);

            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            match cwd {
                Some(dir) => {
                    let result = run_git_pull(&dir, &pane_id).await;
                    let _ = state.broadcast_tx.send(result);
                    // Refresh state after pull
                    let state_result = run_source_control_state(&dir).await;
                    let _ = state.broadcast_tx.send(state_result);
                }
                None => {
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: "Pane has no working directory".to_string(),
                    });
                }
            }
        }

        ClientMessage::UploadFile { pane_id, file_name, data } => {
            info!("Upload file request: {} to pane {}", file_name, pane_id);

            // Get the pane's working directory
            let cwd = if let Some(pane) = state.get_pane(&pane_id).await {
                pane.cwd.clone()
            } else {
                None
            };

            // Default to user's home directory if no cwd is set
            let target_dir = cwd.unwrap_or_else(|| {
                dirs::home_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| ".".to_string())
            });

            // Decode base64 data
            let file_data = match base64_decode(&data) {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to decode base64 file data: {}", e);
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: format!("Failed to decode file data: {}", e),
                    });
                    return Ok(());
                }
            };

            // Construct target file path
            let file_path = std::path::Path::new(&target_dir).join(&file_name);
            let file_path_str = file_path.to_string_lossy().to_string();

            // Write file
            match tokio::fs::write(&file_path, &file_data).await {
                Ok(_) => {
                    info!("File {} uploaded successfully to {}", file_name, file_path_str);
                    let _ = state.broadcast_tx.send(ServerMessage::FileUploaded {
                        pane_id: pane_id.clone(),
                        file_name: file_name.clone(),
                    });
                }
                Err(e) => {
                    error!("Failed to write file {}: {}", file_path_str, e);
                    let _ = state.broadcast_tx.send(ServerMessage::Error {
                        message: format!("Failed to write file: {}", e),
                    });
                }
            }
        }
    }

    Ok(())
}

/// Creates the router with WebSocket and health endpoints.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/tunnel-check", get(tunnel_check_handler))
        .route("/launch", get(launch_handler))
        .route("/proxy", get(proxy_handler))
        .with_state(state)
}

/// Health check handler.
pub async fn health_handler() -> &'static str {
    "OK"
}

/// Tunnel connectivity check handler.
/// Returns JSON response with CORS headers to help frontend establish
/// Dev Tunnel session before attempting WebSocket connection.
/// Dev Tunnels require an initial HTTP request to set cookies/session.
pub async fn tunnel_check_handler() -> Response {
    use axum::http::{header, StatusCode};
    let body = r#"{"status":"ok","ws_ready":true}"#;
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            (header::ACCESS_CONTROL_ALLOW_METHODS, "GET, OPTIONS"),
            (header::ACCESS_CONTROL_ALLOW_HEADERS, "*"),
            (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate"),
        ],
        body,
    ).into_response()
}

/// Launch handler - redirects to frontend with credentials for auto-login.
/// This mimics the "Connect to Mobile" QR code functionality.
pub async fn launch_handler(State(state): State<Arc<AppState>>) -> Response {
    let frontend_url = &state.frontend_url;
    let tunnel_url = &state.tunnel_url;
    let token = &state.auth_token;

    // URL-encode the tunnel URL and token
    let encoded_tunnel = urlencoding::encode(tunnel_url);
    let encoded_token = urlencoding::encode(token);

    let redirect_url = format!("{}/?tunnel={}&token={}", frontend_url, encoded_tunnel, encoded_token);

    info!("Launch redirect to: {}", redirect_url);

    Redirect::to(&redirect_url).into_response()
}

/// Proxy handler - forwards HTTP requests from the browser pane through the backend.
/// This allows the browser pane to access localhost URLs from the viewing device.
pub async fn proxy_handler(
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Response {
    use axum::http::{header, StatusCode, header::HeaderValue};

    let url = match params.get("url") {
        Some(u) => u,
        None => {
            return (StatusCode::BAD_REQUEST, [(header::CONTENT_TYPE, "text/plain")], "Missing url parameter").into_response();
        }
    };

    // Validate the URL is allowed - only allow localhost and local IPs for security
    // Extract host from URL manually (format: http://host:port/path)
    let host = url.trim_start_matches("http://").trim_start_matches("https://").split(':').next().unwrap_or("").split('/').next().unwrap_or("");
    let is_localhost = host == "localhost" || host == "127.0.0.1" || host == "::1";

    // Allow access to localhost, 127.0.0.1, and local network IPs
    let allowed = is_localhost
        || host.starts_with("192.168.")
        || host.starts_with("10.")
        || host.starts_with("172.16.") || host.starts_with("172.17.") || host.starts_with("172.18.")
        || host.starts_with("172.19.") || host.starts_with("172.20.") || host.starts_with("172.21.")
        || host.starts_with("172.22.") || host.starts_with("172.23.") || host.starts_with("172.24.")
        || host.starts_with("172.25.") || host.starts_with("172.26.") || host.starts_with("172.27.")
        || host.starts_with("172.28.") || host.starts_with("172.29.") || host.starts_with("172.30.")
        || host.starts_with("172.31.")
        || host.ends_with(".local");

    if !allowed && !is_localhost {
        warn!("Proxy blocked request to non-local host: {}", url);
        return (StatusCode::FORBIDDEN, [(header::CONTENT_TYPE, "text/plain")], "Only local URLs allowed").into_response();
    }

    // Make the proxied request using the local machine's network
    match reqwest::get(url).await {
        Ok(resp) => {
            let status = resp.status();
            let headers = resp.headers().clone();
            let body = resp.bytes().await;

            match body {
                Ok(body_bytes) => {
                    // Build response headers - filter out problematic ones
                    let mut response_headers = vec![
                        (header::CONTENT_TYPE, HeaderValue::from_str(
                            headers.get(header::CONTENT_TYPE)
                                .map(|v| v.to_str().unwrap_or("text/html"))
                                .unwrap_or("text/html")
                        ).unwrap_or(HeaderValue::from_static("text/html"))),
                        (header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*")),
                    ];

                    // Copy content-length if present
                    if let Some(cl) = headers.get(header::CONTENT_LENGTH) {
                        response_headers.push((header::CONTENT_LENGTH, cl.clone()));
                    }

                    let mut resp = axum::response::Response::new(axum::body::Body::from(body_bytes.to_vec()));
                    *resp.status_mut() = status;
                    for (k, v) in response_headers {
                        resp.headers_mut().insert(k, v);
                    }
                    resp
                }
                Err(e) => {
                    error!("Proxy request body error for {}: {}", url, e);
                    (StatusCode::BAD_GATEWAY, [(header::CONTENT_TYPE, "text/plain")], format!("Upstream error: {}", e)).into_response()
                }
            }
        }
        Err(e) => {
            error!("Proxy request failed for {}: {}", url, e);
            (StatusCode::BAD_GATEWAY, [(header::CONTENT_TYPE, "text/plain")], format!("Connection failed: {}", e)).into_response()
        }
    }
}
