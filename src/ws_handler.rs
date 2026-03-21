//! WebSocket message handling for the terminal multiplexer.
//!
//! Handles client connections, authentication, and message routing.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
    routing::get,
    Router,
};
use futures_util::stream::StreamExt;
use futures_util::sink::SinkExt;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{info, error, warn};

use crate::messages::{ClientMessage, ServerMessage};

use crate::state::AppState;

/// Maximum time to wait for authentication after connection.
const AUTH_TIMEOUT_SECS: u64 = 5;

/// WebSocket upgrade handler for /ws endpoint.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

/// Handles a WebSocket connection from a client.
async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
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
                        let _ = tx.send(ServerMessage::AuthResult {
                            success: true,
                            message: Some("Authenticated".to_string()),
                        }).await;

                        // Send current state to newly authenticated client (via their own channel)
                        let panes = state.get_panes_info().await;
                        let active_panes = state.get_active_panes().await;
                        let floating_panes = state.get_floating_panes().await;
                        let _ = tx.send(ServerMessage::StateUpdate { panes, active_panes: active_panes.clone(), floating_panes: floating_panes.clone() }).await;

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

    // Cleanup
    info!("Cleaning up WebSocket session");
    sender_task.abort();
    broadcast_task.abort();
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
            let panes = state.get_panes_info().await;
            let active_panes = state.get_active_panes().await;
            let floating_panes = state.get_floating_panes().await;
            let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });

            info!("Spawned pane {} with PID {}", pane_id, pid);
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
                let panes = state.get_panes_info().await;
                let active_panes = state.get_active_panes().await;
                let floating_panes = state.get_floating_panes().await;
                let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });
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
            let panes = state.get_panes_info().await;
            let active_panes = state.get_active_panes().await;
            let floating_panes = state.get_floating_panes().await;
            let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });
        }

        ClientMessage::MoveToFloating { pane_id } => {
            info!("Move to floating: {}", pane_id);
            state.move_to_floating(&pane_id).await;

            // Broadcast state update to all clients
            let panes = state.get_panes_info().await;
            let active_panes = state.get_active_panes().await;
            let floating_panes = state.get_floating_panes().await;
            let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });
        }

        ClientMessage::MoveToActive { pane_id } => {
            info!("Move to active: {}", pane_id);
            state.move_to_active(&pane_id).await;

            // Broadcast state update to all clients
            let panes = state.get_panes_info().await;
            let active_panes = state.get_active_panes().await;
            let floating_panes = state.get_floating_panes().await;
            let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });
        }

        ClientMessage::Rename { pane_id, name } => {
            info!("Rename pane {} to {}", pane_id, name);
            state.rename_pane(&pane_id, &name).await;

            // Broadcast state update to all clients
            let panes = state.get_panes_info().await;
            let active_panes = state.get_active_panes().await;
            let floating_panes = state.get_floating_panes().await;
            let _ = state.broadcast_tx.send(ServerMessage::StateUpdate { panes, active_panes, floating_panes });
        }

        ClientMessage::Auth { .. } => {
            // Already handled - shouldn't get here
            warn!("Auth message received after authentication");
        }
    }

    Ok(())
}

/// Creates the router with WebSocket and health endpoints.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .with_state(state)
}

/// Health check handler.
pub async fn health_handler() -> &'static str {
    "OK"
}
