//! Protocol message types for the terminal multiplexer.
//!
//! Defines the bidirectional JSON protocol between client and server.

use serde::{Deserialize, Serialize};
use crate::state::{Pane, PaneGroup};

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
    pub group_id: Option<String>,
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
