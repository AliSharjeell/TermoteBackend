//! Protocol message types for the terminal multiplexer.
//!
//! Defines the bidirectional JSON protocol between client and server.

use serde::{Deserialize, Serialize};
use crate::state::Pane;

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

    /// Kill a specific pane.
    #[serde(rename = "kill")]
    Kill { pane_id: String },

    /// Move a pane to floating tabs.
    #[serde(rename = "move_to_floating")]
    MoveToFloating { pane_id: String },

    /// Move a pane to active grid.
    #[serde(rename = "move_to_active")]
    MoveToActive { pane_id: String },
}

/// Server to client messages.
#[derive(Serialize, Debug)]
#[serde(tag = "event")]
pub enum ServerMessage {
    /// Sent when pane state changes (added/removed/resized).
    #[serde(rename = "state_update")]
    StateUpdate {
        panes: Vec<PaneInfo>,
        active_panes: Vec<String>,
        floating_panes: Vec<String>,
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
    /// Number of columns.
    pub cols: u16,
    /// Number of rows.
    pub rows: u16,
}

impl From<&Pane> for PaneInfo {
    fn from(pane: &Pane) -> Self {
        PaneInfo {
            id: pane.id.clone(),
            pid: pane.pid,
            shell: pane.shell.clone(),
            cols: pane.cols,
            rows: pane.rows,
        }
    }
}
