//! Application state management for the terminal multiplexer.
//!
//! Manages panes, authentication state, and WebSocket sessions.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::messages::{PaneInfo, PaneGroupInfo};
use crate::pty_manager::PtyManager;

/// Represents a connected device/client session.
#[derive(Clone, Debug)]
pub struct ConnectedDevice {
    /// Unique connection ID
    pub id: String,
    /// Client's IP address
    pub ip: String,
    /// User-Agent string from the client
    pub user_agent: String,
    /// When this connection was established (Unix timestamp)
    pub connected_at: u64,
    /// Whether this device is currently authenticated
    pub authenticated: bool,
}

impl ConnectedDevice {
    /// Creates a new ConnectedDevice.
    pub fn new(ip: String, user_agent: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            ip,
            user_agent,
            connected_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            authenticated: false,
        }
    }

    /// Returns a human-readable device info string derived from User-Agent.
    pub fn device_info(&self) -> String {
        let ua = &self.user_agent.to_lowercase();
        if ua.contains("chrome") && ua.contains("windows") {
            "Windows + Chrome".to_string()
        } else if ua.contains("firefox") && ua.contains("windows") {
            "Windows + Firefox".to_string()
        } else if ua.contains("safari") && ua.contains("mac") && !ua.contains("chrome") {
            "macOS + Safari".to_string()
        } else if ua.contains("chrome") && ua.contains("mac") {
            "macOS + Chrome".to_string()
        } else if ua.contains("chrome") && ua.contains("android") {
            "Android + Chrome".to_string()
        } else if ua.contains("mobile") && ua.contains("safari") {
            "iPhone/iPad + Safari".to_string()
        } else if ua.contains("linux") {
            "Linux".to_string()
        } else if self.user_agent.is_empty() {
            "Unknown".to_string()
        } else {
            // Return first 50 chars of user agent as fallback
            let truncated = if self.user_agent.len() > 50 {
                format!("{}...", &self.user_agent[..50])
            } else {
                self.user_agent.clone()
            };
            truncated
        }
    }
}

/// Represents a historical connection entry.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConnectionHistoryEntry {
    pub ip: String,
    pub user_agent: String,
    pub connected_at: u64,
    pub disconnected_at: Option<u64>,
    pub was_banned: bool,
}

/// Security state containing ban list and connection history.
#[derive(Clone, Debug)]
pub struct SecurityState {
    /// List of banned IP addresses
    pub banned_ips: Vec<String>,
    /// Connection history
    pub history: Vec<ConnectionHistoryEntry>,
    /// Path to the security JSON file
    config_path: PathBuf,
}

impl SecurityState {
    /// Creates a new SecurityState, loading from disk if exists.
    pub fn new(config_dir: PathBuf) -> Self {
        let config_path = config_dir.join("security.json");
        let (banned_ips, history) = if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(contents) => {
                    match serde_json::from_str::<serde_json::Value>(&contents) {
                        Ok(json) => {
                            let banned_ips = json.get("banned_ips")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                                .unwrap_or_default();
                            let history = json.get("history")
                                .and_then(|v| v.as_array())
                                .map(|arr| arr.iter().filter_map(|v| serde_json::from_value(v.clone()).ok()).collect())
                                .unwrap_or_default();
                            (banned_ips, history)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to parse security.json: {}", e);
                            (Vec::new(), Vec::new())
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read security.json: {}", e);
                    (Vec::new(), Vec::new())
                }
            }
        } else {
            (Vec::new(), Vec::new())
        };

        Self {
            banned_ips,
            history,
            config_path,
        }
    }

    /// Adds an IP to the ban list and persists to disk.
    pub fn ban_ip(&mut self, ip: &str) {
        if !self.banned_ips.contains(&ip.to_string()) {
            self.banned_ips.push(ip.to_string());
            self.save();
        }
    }

    /// Removes an IP from the ban list and persists to disk.
    pub fn unban_ip(&mut self, ip: &str) -> bool {
        let was_present = self.banned_ips.contains(&ip.to_string());
        self.banned_ips.retain(|i| i != ip);
        if was_present {
            self.save();
        }
        was_present
    }

    /// Checks if an IP is banned.
    pub fn is_banned(&self, ip: &str) -> bool {
        self.banned_ips.contains(&ip.to_string())
    }

    /// Adds a connection to history.
    pub fn add_connection(&mut self, ip: &str, user_agent: &str, connected_at: u64) {
        self.history.push(ConnectionHistoryEntry {
            ip: ip.to_string(),
            user_agent: user_agent.to_string(),
            connected_at,
            disconnected_at: None,
            was_banned: false,
        });
        // Keep only last 1000 history entries
        if self.history.len() > 1000 {
            self.history = self.history.split_off(self.history.len() - 1000);
        }
        self.save();
    }

    /// Marks a connection as disconnected in history.
    pub fn mark_disconnected(&mut self, ip: &str, disconnected_at: u64, was_banned: bool) {
        // Find the most recent entry for this IP that hasn't been disconnected yet
        if let Some(entry) = self.history.iter_mut().rev()
            .find(|e| e.ip == ip && e.disconnected_at.is_none())
        {
            entry.disconnected_at = Some(disconnected_at);
            entry.was_banned = was_banned;
            self.save();
        }
    }

    /// Saves state to disk.
    fn save(&self) {
        if let Some(parent) = self.config_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let json = serde_json::json!({
            "banned_ips": &self.banned_ips,
            "history": &self.history,
        });
        if let Ok(contents) = serde_json::to_string_pretty(&json) {
            let _ = fs::write(&self.config_path, contents);
        }
    }
}

/// A single terminal pane with its associated PTY and state.
#[derive(Clone)]
pub struct Pane {
    /// Unique identifier for the pane.
    pub id: String,
    /// Process ID of the spawned shell.
    pub pid: u32,
    /// Shell program name.
    pub shell: String,
    /// Display name (editable by user).
    pub name: String,
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
    /// Scrollback buffer - recent output history
    pub buffer: Vec<u8>,
    /// Group ID this pane belongs to (None if ungrouped).
    pub group_id: Option<String>,
}

impl Pane {
    /// Creates a new Pane with a generated UUID.
    pub fn new(pid: u32, shell: String, cols: u16, rows: u16) -> Self {
        let name = format!("{} ({})", shell, pid);
        Self {
            id: Uuid::new_v4().to_string(),
            pid,
            shell,
            name,
            cols,
            rows,
            buffer: Vec::new(),
            group_id: None,
        }
    }

    /// Appends data to the scrollback buffer, capping at MAX_BUFFER_SIZE bytes.
    pub fn append_buffer(&mut self, data: &[u8]) {
        const MAX_BUFFER_SIZE: usize = 1_000_000;
        self.buffer.extend_from_slice(data);
        // Cap buffer size
        if self.buffer.len() > MAX_BUFFER_SIZE {
            let excess = self.buffer.len() - MAX_BUFFER_SIZE;
            self.buffer.drain(0..excess);
        }
    }
}

/// A group of panes with a name and color.
#[derive(Clone, Debug)]
pub struct PaneGroup {
    /// Unique identifier for the group.
    pub id: String,
    /// Display name of the group.
    pub name: String,
    /// Hex color code for the group.
    pub color: String,
}

impl PaneGroup {
    /// Creates a new PaneGroup with a generated UUID.
    pub fn new(name: String, color: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            color,
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
    /// Frontend URL for redirect (e.g., "https://termote.vercel.app")
    pub frontend_url: String,
    /// Tunnel URL (public WebSocket URL of this server)
    pub tunnel_url: String,
    /// Broadcast channel for terminal output (Radio Tower)
    pub broadcast_tx: Arc<broadcast::Sender<crate::messages::ServerMessage>>,
    /// Shared PTY manager for all WebSocket connections
    pub pty_manager: Arc<PtyManager>,
    /// Map of group ID to PaneGroup.
    pub groups: Arc<RwLock<HashMap<String, PaneGroup>>>,
    /// Cold start: initial directory to spawn first terminal at
    pub cold_start_dir: Option<String>,
    /// Currently connected devices
    pub connected_devices: Arc<RwLock<HashMap<String, ConnectedDevice>>>,
    /// Security state (ban list, history)
    pub security: Arc<RwLock<SecurityState>>,
}

impl AppState {
    /// Creates a new AppState with the given auth token.
    pub fn new(auth_token: String, frontend_url: String, tunnel_url: String, cold_start_dir: Option<String>) -> Self {
        let (broadcast_tx, _) = broadcast::channel(100);

        // Initialize config directory for security state
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("termote");

        Self {
            panes: Arc::new(RwLock::new(HashMap::new())),
            active_panes: Arc::new(RwLock::new(Vec::new())),
            floating_panes: Arc::new(RwLock::new(Vec::new())),
            authenticated: Arc::new(RwLock::new(false)),
            auth_token,
            frontend_url,
            tunnel_url,
            broadcast_tx: Arc::new(broadcast_tx),
            pty_manager: Arc::new(PtyManager::new()),
            groups: Arc::new(RwLock::new(HashMap::new())),
            cold_start_dir,
            connected_devices: Arc::new(RwLock::new(HashMap::new())),
            security: Arc::new(RwLock::new(SecurityState::new(config_dir))),
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

    /// Renames a pane.
    pub async fn rename_pane(&self, pane_id: &str, name: &str) -> bool {
        let mut panes = self.panes.write().await;
        if let Some(pane) = panes.get_mut(pane_id) {
            pane.name = name.to_string();
            true
        } else {
            false
        }
    }

    /// Sets a pane's group.
    pub async fn set_pane_group(&self, pane_id: &str, group_id: Option<&str>) -> bool {
        let mut panes = self.panes.write().await;
        if let Some(pane) = panes.get_mut(pane_id) {
            pane.group_id = group_id.map(|s| s.to_string());
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

    /// Adds a new group.
    pub async fn add_group(&self, group: PaneGroup) {
        let mut groups = self.groups.write().await;
        groups.insert(group.id.clone(), group);
    }

    /// Removes a group and ungroups all panes in it.
    pub async fn remove_group(&self, group_id: &str) -> bool {
        let mut groups = self.groups.write().await;
        if groups.remove(group_id).is_some() {
            // Ungroup all panes in this group
            let mut panes = self.panes.write().await;
            for pane in panes.values_mut() {
                if pane.group_id.as_deref() == Some(group_id) {
                    pane.group_id = None;
                }
            }
            true
        } else {
            false
        }
    }

    /// Gets a group by ID.
    pub async fn get_group(&self, group_id: &str) -> Option<PaneGroup> {
        let groups = self.groups.read().await;
        groups.get(group_id).cloned()
    }

    /// Gets all groups as PaneGroupInfo structs.
    pub async fn get_all_groups(&self) -> Vec<PaneGroupInfo> {
        let groups = self.groups.read().await;
        groups.values().map(|g| PaneGroupInfo {
            id: g.id.clone(),
            name: g.name.clone(),
            color: g.color.clone(),
        }).collect()
    }

    /// Renames a group.
    pub async fn rename_group(&self, group_id: &str, name: &str) -> bool {
        let mut groups = self.groups.write().await;
        if let Some(group) = groups.get_mut(group_id) {
            group.name = name.to_string();
            true
        } else {
            false
        }
    }

    /// Spawns a new pane with the shell starting in the specified directory.
    /// Returns the pane_id if successful.
    pub async fn spawn_pane_at_dir(
        &self,
        dir: &str,
        shell: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let shell_to_use = if shell.is_empty() { "powershell.exe" } else { shell };

        // Spawn the PTY
        let (pane_id, pid) = self.pty_manager.spawn_pty(
            shell_to_use,
            80,
            24,
            self.clone(),
            &self.broadcast_tx,
        )?;

        // Create pane with correct ID
        let mut pane = Pane::new(pid, shell_to_use.to_string(), 80, 24);
        pane.id = pane_id.clone();
        self.add_pane(pane).await;

        // Wait a bit for shell to start, then cd to directory
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Send cd command to the pane
        // Windows needs \r\n (carriage return + newline) to execute the command
        #[cfg(windows)]
        let cd_cmd = format!("cd '{}'; Clear-Host\r\n", dir.replace("'", "''"));
        #[cfg(not(windows))]
        let cd_cmd = format!("cd '{}'; clear\r\n", dir.replace("'", "\\'"));

        if let Err(e) = self.pty_manager.write_input_raw(&pane_id, &cd_cmd) {
            tracing::error!("Failed to cd to directory {}: {}", dir, e);
        }

        // Broadcast state update
        let panes = self.get_panes_info().await;
        let active_panes = self.get_active_panes().await;
        let floating_panes = self.get_floating_panes().await;
        let groups = self.get_all_groups().await;
        let _ = self.broadcast_tx.send(crate::messages::ServerMessage::StateUpdate {
            panes,
            active_panes,
            floating_panes,
            groups,
        });

        tracing::info!("Spawned pane {} at directory {}", pane_id, dir);
        Ok(pane_id)
    }

    // ==================== Device Management ====================

    /// Adds a connected device to tracking.
    pub async fn add_device(&self, device: ConnectedDevice) {
        let mut devices = self.connected_devices.write().await;
        devices.insert(device.id.clone(), device.clone());
        // Add to history
        let mut security = self.security.write().await;
        security.add_connection(&device.ip, &device.user_agent, device.connected_at);
    }

    /// Removes a connected device and marks disconnected in history.
    pub async fn remove_device(&self, device_id: &str) {
        let mut devices = self.connected_devices.write().await;
        if let Some(device) = devices.remove(device_id) {
            let disconnected_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut security = self.security.write().await;
            security.mark_disconnected(&device.ip, disconnected_at, false);
        }
    }

    /// Marks a device as disconnected (due to ban).
    pub async fn remove_device_banned(&self, device_id: &str) {
        let mut devices = self.connected_devices.write().await;
        if let Some(device) = devices.remove(device_id) {
            let disconnected_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let mut security = self.security.write().await;
            security.mark_disconnected(&device.ip, disconnected_at, true);
        }
    }

    /// Gets all connected devices.
    pub async fn get_devices(&self) -> Vec<ConnectedDevice> {
        let devices = self.connected_devices.read().await;
        devices.values().cloned().collect()
    }

    /// Gets all connected devices (alias for get_devices).
    pub async fn get_connected_devices(&self) -> Vec<ConnectedDevice> {
        self.get_devices().await
    }

    /// Gets connection history.
    pub async fn get_connection_history(&self) -> Vec<ConnectionHistoryEntry> {
        let security = self.security.read().await;
        security.history.clone()
    }

    /// Gets the device ID for a given IP address (if connected).
    pub async fn get_device_by_ip(&self, ip: &str) -> Option<String> {
        let devices = self.connected_devices.read().await;
        devices.values().find(|d| d.ip == ip).map(|d| d.id.clone())
    }

    /// Bans an IP address and kicks all devices from that IP.
    pub async fn ban_ip(&self, ip: &str) {
        // First ban the IP
        {
            let mut security = self.security.write().await;
            security.ban_ip(ip);
        }

        // Find and remove all devices from this IP
        let devices_to_remove: Vec<String> = {
            let devices = self.connected_devices.read().await;
            devices.values()
                .filter(|d| d.ip == ip)
                .map(|d| d.id.clone())
                .collect()
        };

        for device_id in devices_to_remove {
            self.remove_device_banned(&device_id).await;
        }
    }

    /// Unbans an IP address.
    pub async fn unban_ip(&self, ip: &str) -> bool {
        let mut security = self.security.write().await;
        security.unban_ip(ip)
    }

    /// Gets list of banned IPs.
    pub async fn get_banned_ips(&self) -> Vec<String> {
        let security = self.security.read().await;
        security.banned_ips.clone()
    }

    /// Checks if an IP is banned.
    pub async fn is_ip_banned(&self, ip: &str) -> bool {
        let security = self.security.read().await;
        security.is_banned(ip)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_app_state_pane_management() {
        let state = AppState::new("test_token".to_string(), "http://localhost".to_string(), "ws://localhost".to_string(), None);

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
        let state = AppState::new("secret123".to_string(), "http://localhost".to_string(), "ws://localhost".to_string(), None);

        assert!(!state.is_authenticated().await);
        assert!(state.validate_token("wrong").await);
        assert!(!state.validate_token("wrong").await);
        assert!(state.validate_token("secret123").await);
    }
}
