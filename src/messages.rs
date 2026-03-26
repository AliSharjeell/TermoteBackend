//! Protocol message types for the terminal multiplexer.
//!
//! Defines the bidirectional JSON protocol between client and server.

use serde::{Deserialize, Serialize};
use crate::state::{ConnectedDevice, Pane, PaneGroup};

/// Client to server messages.
#[derive(Deserialize, Debug)]
#[serde(tag = "action")]
pub enum ClientMessage {
    /// Authenticate with the server using a token.
    #[serde(rename = "auth")]
    Auth { token: String },

    /// Spawn a new terminal pane with the specified shell.
    #[serde(rename = "spawn")]
    Spawn { shell: String },

    /// Spawn a new terminal pane at a specific directory.
    #[serde(rename = "spawn_at_dir")]
    SpawnAtDir { shell: String, dir: String },

    /// Send input data to a specific pane.
    #[serde(rename = "input")]
    Input { pane_id: String, data: String },

    /// Resize a specific pane.
    #[serde(rename = "resize")]
    Resize { pane_id: String, cols: u16, rows: u16 },

    /// Force refocus - ignores circuit breaker, broadcasts to all clients.
    /// Used when switching devices to make this client's size take priority.
    #[serde(rename = "refocus")]
    Refocus { pane_id: String, cols: u16, rows: u16 },

    /// Kill a specific pane.
    #[serde(rename = "kill")]
    Kill { pane_id: String },

    /// Move a pane to floating tabs.
    #[serde(rename = "move_to_floating")]
    MoveToFloating { pane_id: String },

    /// Move a pane to active grid.
    #[serde(rename = "move_to_active")]
    MoveToActive { pane_id: String },

    /// Rename a pane.
    #[serde(rename = "rename")]
    Rename { pane_id: String, name: String },

    /// Ping/pong heartbeat (no-op, just keeps connection alive).
    #[serde(rename = "ping")]
    Ping,

    /// Create a new pane group.
    #[serde(rename = "create_group")]
    CreateGroup { id: Option<String>, name: String, color: String },

    /// Delete a pane group (panes in group become ungrouped).
    #[serde(rename = "delete_group")]
    DeleteGroup { group_id: String },

    /// Rename a pane group.
    #[serde(rename = "rename_group")]
    RenameGroup { group_id: String, name: String },

    /// Set a pane's group (or null to remove from group).
    #[serde(rename = "set_pane_group")]
    SetPaneGroup { pane_id: String, group_id: Option<String> },

    /// Request a native directory picker dialog.
    /// Server opens OS dialog, spawns terminal at selected directory.
    #[serde(rename = "request_directory_picker")]
    RequestDirectoryPicker { shell: String },

    /// List directory contents at the given path.
    /// Returns drives (C:\, D:\, etc.) if path is empty/null.
    #[serde(rename = "list_directory")]
    ListDirectory { path: Option<String> },

    /// Request the list of connected devices.
    #[serde(rename = "get_device_list")]
    GetDeviceList,

    /// Kick a connected device (forced disconnect).
    #[serde(rename = "kick_device")]
    KickDevice { device_id: String },

    /// Ban an IP address from connecting.
    #[serde(rename = "ban_device")]
    BanDevice { ip: String },

    /// Upload a file to a pane's current working directory.
    #[serde(rename = "upload_file")]
    UploadFile { pane_id: String, file_name: String, data: String },
}

/// Server to client messages.
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "event")]
pub enum ServerMessage {
    /// Sent when pane state changes (added/removed/resized).
    #[serde(rename = "state_update")]
    StateUpdate {
        panes: Vec<PaneInfo>,
        active_panes: Vec<String>,
        floating_panes: Vec<String>,
        groups: Vec<PaneGroupInfo>,
    },

    /// Terminal output data from a pane.
    #[serde(rename = "output")]
    Output { pane_id: String, data: String },

    /// Authentication result.
    #[serde(rename = "auth_result")]
    AuthResult {
        success: bool,
        message: Option<String>,
    },

    /// A group was created.
    #[serde(rename = "group_created")]
    GroupCreated { group: PaneGroupInfo },

    /// A group was deleted.
    #[serde(rename = "group_deleted")]
    GroupDeleted { group_id: String },

    /// A group was renamed.
    #[serde(rename = "group_renamed")]
    GroupRenamed { group_id: String, name: String },

    /// A pane's group was set.
    #[serde(rename = "pane_group_set")]
    PaneGroupSet { pane_id: String, group_id: Option<String> },

    /// Directory picker was cancelled by user.
    #[serde(rename = "directory_picker_cancelled")]
    DirectoryPickerCancelled,

    /// Directory contents response for file explorer.
    #[serde(rename = "directory_contents")]
    DirectoryContents {
        path: String,
        items: Vec<DirectoryItem>,
    },

    /// List of connected devices (sent in response to get_device_list).
    #[serde(rename = "device_list")]
    DeviceList { devices: Vec<DeviceInfo> },

    /// A device was kicked (forced disconnect).
    #[serde(rename = "device_kicked")]
    DeviceKicked { device_id: String },

    /// A device was banned.
    #[serde(rename = "device_banned")]
    DeviceBanned { ip: String },

    /// An error occurred (e.g., device not found).
    #[serde(rename = "error")]
    Error { message: String },

    /// A file was successfully uploaded to a pane's directory.
    #[serde(rename = "file_uploaded")]
    FileUploaded { pane_id: String, file_name: String },
}

/// Information about a connected device sent to clients.
#[derive(Serialize, Clone, Debug)]
pub struct DeviceInfo {
    /// Unique connection ID
    pub id: String,
    /// Client's IP address
    pub ip: String,
    /// Inferred device/browser info from User-Agent
    pub device: String,
    /// When this connection was established (Unix timestamp)
    pub connected_at: u64,
    /// Whether this device is currently authenticated
    pub authenticated: bool,
}

impl From<&ConnectedDevice> for DeviceInfo {
    fn from(device: &ConnectedDevice) -> Self {
        DeviceInfo {
            id: device.id.clone(),
            ip: device.ip.clone(),
            device: device.device_info(),
            connected_at: device.connected_at,
            authenticated: device.authenticated,
        }
    }
}

/// Information about a pane sent to clients.
#[derive(Serialize, Clone, Debug)]
pub struct PaneInfo {
    /// Unique identifier for the pane.
    pub id: String,
    /// Process ID of the shell.
    pub pid: u32,
    /// Shell program name.
    pub shell: String,
    /// Display name.
    pub name: String,
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
    /// Group ID this pane belongs to (null if ungrouped).
    #[serde(rename = "groupId")]
    pub group_id: Option<String>,
    /// Current working directory of the pane's shell.
    pub cwd: Option<String>,
}

impl From<&Pane> for PaneInfo {
    fn from(pane: &Pane) -> Self {
        PaneInfo {
            id: pane.id.clone(),
            pid: pane.pid,
            shell: pane.shell.clone(),
            name: pane.name.clone(),
            cols: pane.cols,
            rows: pane.rows,
            group_id: pane.group_id.clone(),
            cwd: pane.cwd.clone(),
        }
    }
}

/// Information about a pane group sent to clients.
#[derive(Serialize, Clone, Debug)]
pub struct PaneGroupInfo {
    /// Unique identifier for the group.
    pub id: String,
    /// Display name of the group.
    pub name: String,
    /// Hex color code for the group.
    pub color: String,
}

impl From<&PaneGroup> for PaneGroupInfo {
    fn from(group: &PaneGroup) -> Self {
        PaneGroupInfo {
            id: group.id.clone(),
            name: group.name.clone(),
            color: group.color.clone(),
        }
    }
}

/// A single item in a directory listing.
#[derive(Serialize, Clone, Debug)]
pub struct DirectoryItem {
    /// Name of the file or directory.
    pub name: String,
    /// Absolute path to the item.
    pub absolute_path: String,
    /// Whether this item is a directory.
    pub is_dir: bool,
}
