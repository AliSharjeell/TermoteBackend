//! WebSocket message handling for the terminal multiplexer.
//!
//! Handles client connections, authentication, and message routing.

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
        }

        ClientMessage::MoveToFloating { pane_id } => {
            info!("Move to floating: {}", pane_id);
            state.move_to_floating(&pane_id).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
        }

        ClientMessage::MoveToActive { pane_id } => {
            info!("Move to active: {}", pane_id);
            state.move_to_active(&pane_id).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
        }

        ClientMessage::Rename { pane_id, name } => {
            info!("Rename pane {} to {}", pane_id, name);
            state.rename_pane(&pane_id, &name).await;

            // Broadcast state update to all clients
            broadcast_state_update(state).await;
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
        }

        ClientMessage::DeleteGroup { group_id } => {
            info!("Delete group: {}", group_id);
            if state.remove_group(&group_id).await {
                // Broadcast group deleted event
                let _ = state.broadcast_tx.send(ServerMessage::GroupDeleted { group_id: group_id.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            }
        }

        ClientMessage::RenameGroup { group_id, name } => {
            info!("Rename group {} to {}", group_id, name);
            if state.rename_group(&group_id, &name).await {
                // Broadcast group renamed event
                let _ = state.broadcast_tx.send(ServerMessage::GroupRenamed { group_id: group_id.clone(), name: name.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            }
        }

        ClientMessage::SetPaneGroup { pane_id, group_id } => {
            info!("Set pane {} group to {:?}", pane_id, group_id);
            if state.set_pane_group(&pane_id, group_id.as_deref()).await {
                // Broadcast pane group set event
                let _ = state.broadcast_tx.send(ServerMessage::PaneGroupSet { pane_id: pane_id.clone(), group_id: group_id.clone() });

                // Broadcast state update to sync groups
                broadcast_state_update(state).await;
            }
        }

        ClientMessage::GetDeviceList => {
            info!("Device list requested");
            let devices = state.get_connected_devices().await;
            let device_infos: Vec<DeviceInfo> = devices.iter().map(DeviceInfo::from).collect();
            let _ = state.broadcast_tx.send(ServerMessage::DeviceList { devices: device_infos });
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
        .route("/launch", get(launch_handler))
        .with_state(state)
}

/// Health check handler.
pub async fn health_handler() -> &'static str {
    "OK"
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
