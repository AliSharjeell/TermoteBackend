//! Application state management for the terminal multiplexer.
//!
//! Manages panes, authentication state, and WebSocket sessions.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::messages::PaneInfo;
use crate::pty_manager::PtyManager;

/// A single terminal pane with its associated PTY and state.
#[derive(Clone)]
pub struct Pane {
    /// Unique identifier for the pane.
    pub id: String,
    /// Process ID of the spawned shell.
    pub pid: u32,
    /// Shell program name.
    pub shell: String,
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
    /// Scrollback buffer - recent output history
    pub buffer: Vec<u8>,
}

impl Pane {
    /// Creates a new Pane with a generated UUID.
    pub fn new(pid: u32, shell: String, cols: u16, rows: u16) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            pid,
            shell,
            cols,
            rows,
            buffer: Vec::new(),
        }
    }

    /// Appends data to the scrollback buffer, capping at MAX_BUFFER_SIZE bytes.
    pub fn append_buffer(&mut self, data: &[u8]) {
        const MAX_BUFFER_SIZE: usize = 50_000;
        self.buffer.extend_from_slice(data);
        // Cap buffer size
        if self.buffer.len() > MAX_BUFFER_SIZE {
            let excess = self.buffer.len() - MAX_BUFFER_SIZE;
            self.buffer.drain(0..excess);
        }
    }
}

/// Global application state shared across all WebSocket connections.
#[derive(Clone)]
pub struct AppState {
    /// Map of pane ID to Pane struct.
    pub panes: Arc<RwLock<HashMap<String, Pane>>>,
    /// IDs of panes in the main grid view.
    pub active_panes: Arc<RwLock<Vec<String>>>,
    /// IDs of panes extracted to floating tabs.
    pub floating_panes: Arc<RwLock<Vec<String>>>,
    /// Whether a client has authenticated.
    pub authenticated: Arc<RwLock<bool>>,
    /// Expected authentication token.
    pub auth_token: String,
    /// Broadcast channel for terminal output (Radio Tower)
    pub broadcast_tx: Arc<broadcast::Sender<crate::messages::ServerMessage>>,
    /// Shared PTY manager for all WebSocket connections
    pub pty_manager: Arc<PtyManager>,
}

impl AppState {
    /// Creates a new AppState with the given auth token.
    pub fn new(auth_token: String) -> Self {
        let (broadcast_tx, _) = broadcast::channel(100);
        Self {
            panes: Arc::new(RwLock::new(HashMap::new())),
            active_panes: Arc::new(RwLock::new(Vec::new())),
            floating_panes: Arc::new(RwLock::new(Vec::new())),
            authenticated: Arc::new(RwLock::new(false)),
            auth_token,
            broadcast_tx: Arc::new(broadcast_tx),
            pty_manager: Arc::new(PtyManager::new()),
        }
    }

    /// Adds a new pane to the state and puts it in active_panes.
    pub async fn add_pane(&self, pane: Pane) {
        let pane_id = pane.id.clone();
        let mut panes = self.panes.write().await;
        panes.insert(pane_id.clone(), pane);

        let mut active = self.active_panes.write().await;
        if !active.contains(&pane_id) {
            active.push(pane_id);
        }
    }

    /// Removes a pane from the state.
    pub async fn remove_pane(&self, pane_id: &str) -> Option<Pane> {
        let mut panes = self.panes.write().await;
        let result = panes.remove(pane_id);

        // Clean up layout arrays
        let mut active = self.active_panes.write().await;
        let mut floating = self.floating_panes.write().await;
        active.retain(|id| id != pane_id);
        floating.retain(|id| id != pane_id);

        result
    }

    /// Gets a pane by ID.
    pub async fn get_pane(&self, pane_id: &str) -> Option<Pane> {
        let panes = self.panes.read().await;
        panes.get(pane_id).cloned()
    }

    /// Gets all panes as PaneInfo structs.
    pub async fn get_panes_info(&self) -> Vec<PaneInfo> {
        let panes = self.panes.read().await;
        panes.values().map(PaneInfo::from).collect()
    }

    /// Gets active pane IDs.
    pub async fn get_active_panes(&self) -> Vec<String> {
        let active = self.active_panes.read().await;
        active.clone()
    }

    /// Gets floating pane IDs.
    pub async fn get_floating_panes(&self) -> Vec<String> {
        let floating = self.floating_panes.read().await;
        floating.clone()
    }

    /// Appends data to a pane's scrollback buffer.
    pub async fn append_pane_buffer(&self, pane_id: &str, data: &[u8]) {
        let mut panes = self.panes.write().await;
        if let Some(pane) = panes.get_mut(pane_id) {
            pane.append_buffer(data);
        }
    }

    /// Gets a pane's scrollback buffer.
    pub async fn get_pane_buffer(&self, pane_id: &str) -> Vec<u8> {
        let panes = self.panes.read().await;
        if let Some(pane) = panes.get(pane_id) {
            pane.buffer.clone()
        } else {
            Vec::new()
        }
    }

    /// Gets scrollback buffers for multiple panes.
    pub async fn get_panes_buffers(&self, pane_ids: &[String]) -> Vec<(String, Vec<u8>)> {
        let mut result = Vec::new();
        for pane_id in pane_ids {
            let buffer = self.get_pane_buffer(pane_id).await;
            result.push((pane_id.clone(), buffer));
        }
        result
    }

    /// Moves a pane to floating tabs.
    pub async fn move_to_floating(&self, pane_id: &str) {
        let mut active = self.active_panes.write().await;
        let mut floating = self.floating_panes.write().await;

        active.retain(|id| id != pane_id);
        if !floating.contains(&pane_id.to_string()) {
            floating.push(pane_id.to_string());
        }
    }

    /// Moves a pane to active grid.
    pub async fn move_to_active(&self, pane_id: &str) {
        let mut active = self.active_panes.write().await;
        let mut floating = self.floating_panes.write().await;

        floating.retain(|id| id != pane_id);
        if !active.contains(&pane_id.to_string()) {
            active.push(pane_id.to_string());
        }
    }

    /// Updates pane dimensions.
    pub async fn resize_pane(&self, pane_id: &str, cols: u16, rows: u16) -> bool {
        let mut panes = self.panes.write().await;
        if let Some(pane) = panes.get_mut(pane_id) {
            pane.cols = cols;
            pane.rows = rows;
            true
        } else {
            false
        }
    }

    /// Checks if the given token is valid.
    pub async fn validate_token(&self, token: &str) -> bool {
        &self.auth_token == token
    }

    /// Sets the authenticated flag.
    pub async fn set_authenticated(&self, value: bool) {
        let mut auth = self.authenticated.write().await;
        *auth = value;
    }

    /// Checks if a client is authenticated.
    pub async fn is_authenticated(&self) -> bool {
        let auth = self.authenticated.read().await;
        *auth
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_pane_management() {
        let state = AppState::new("test_token".to_string());

        // Initially empty
        assert!(state.get_panes_info().await.is_empty());

        // Add a pane
        let pane = Pane::new(1234, "powershell.exe".to_string(), 80, 24);
        let pane_id = pane.id.clone();
        state.add_pane(pane).await;

        // Should have one pane
        let panes = state.get_panes_info().await;
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].pid, 1234);

        // Remove the pane
        let removed = state.remove_pane(&pane_id).await;
        assert!(removed.is_some());
        assert!(state.get_panes_info().await.is_empty());
    }

    #[tokio::test]
    async fn test_authentication() {
        let state = AppState::new("secret123".to_string());

        assert!(!state.is_authenticated().await);
        assert!(state.validate_token("wrong").await);
        assert!(!state.validate_token("wrong").await);
        assert!(state.validate_token("secret123").await);
    }
}
