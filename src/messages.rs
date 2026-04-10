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

    /// Spawn a browser tab at a specific URL.
    #[serde(rename = "spawn_browser")]
    SpawnBrowser { url: String },

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

    /// Get git status for a pane's directory.
    #[serde(rename = "get_git_status")]
    GetGitStatus { pane_id: String },

    /// Commit staged changes with a message.
    #[serde(rename = "git_commit")]
    GitCommit { pane_id: String, message: String },

    /// Stage or unstage files (git add / git reset).
    #[serde(rename = "git_stage")]
    GitStage { pane_id: String, files: Vec<String>, unstage: bool },

    /// Push commits to remote.
    #[serde(rename = "git_push")]
    GitPush { pane_id: String },

    /// Pull commits from remote.
    #[serde(rename = "git_pull")]
    GitPull { pane_id: String },

    /// Get git log / commit history.
    #[serde(rename = "git_log")]
    GitLog { pane_id: String },

    /// Get full source control state for a directory.
    #[serde(rename = "get_source_control_state")]
    GetSourceControlState { path: String },

    /// Find all git repositories in subdirectories.
    #[serde(rename = "find_git_repos")]
    FindGitRepos { path: String },

    /// Get list of processes running on ports.
    #[serde(rename = "get_port_processes")]
    GetPortProcesses,
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

    /// Git status response for a directory.
    #[serde(rename = "git_status")]
    GitStatus {
        pane_id: String,
        dir: String,
        is_repo: bool,
        branch: Option<String>,
        staged: Vec<String>,
        unstaged: Vec<String>,
        untracked: Vec<String>,
        ahead: Option<i32>,
        behind: Option<i32>,
    },

    /// Git commit result.
    #[serde(rename = "git_commit_result")]
    GitCommitResult { pane_id: String, success: bool, message: String },

    /// Git log / commit history.
    #[serde(rename = "git_log")]
    GitLog {
        pane_id: String,
        dir: String,
        commits: Vec<GitCommitInfo>,
    },

    /// Full source control state.
    #[serde(rename = "source_control_state")]
    SourceControlState {
        path: String,
        is_repo: bool,
        branch: Option<String>,
        remote: Option<String>,
        staged: Vec<SourceControlFile>,
        unstaged: Vec<SourceControlFile>,
        untracked: Vec<SourceControlFile>,
        ahead: i32,
        behind: i32,
        outgoing_commits: Vec<OutgoingCommit>,
    },

    /// Git repositories found in subdirectories.
    #[serde(rename = "git_repos_found")]
    GitReposFound {
        repos: Vec<GitRepoInfo>,
    },

    /// List of processes on ports.
    #[serde(rename = "port_processes")]
    PortProcesses {
        processes: Vec<PortProcess>,
    },
}

/// A git repository info.
#[derive(Serialize, Clone, Debug)]
pub struct GitRepoInfo {
    pub path: String,
    pub name: String,
    pub branch: Option<String>,
}

/// A file in source control.
#[derive(Serialize, Clone, Debug)]
pub struct SourceControlFile {
    pub path: String,
    pub status: String,
    pub added: Option<u32>,
    pub deleted: Option<u32>,
}

/// A commit that hasn't been pushed.
#[derive(Serialize, Clone, Debug)]
pub struct OutgoingCommit {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
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
#[derive(Serialize, Deserialize, Clone, Debug)]
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
#[derive(Serialize, Deserialize, Clone, Debug)]
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

impl From<PaneGroupInfo> for PaneGroup {
    fn from(info: PaneGroupInfo) -> Self {
        PaneGroup {
            id: info.id,
            name: info.name,
            color: info.color,
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

/// A single commit in git log.
#[derive(Serialize, Clone, Debug)]
pub struct GitCommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub message: String,
    pub author: String,
    pub date: String,
}

/// A process running on a port.
#[derive(Serialize, Clone, Debug)]
pub struct PortProcess {
    pub port: u16,
    pub pid: u32,
    pub process_name: String,
    pub cwd: Option<String>,
}
