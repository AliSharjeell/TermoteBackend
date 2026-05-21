//! Termote backend library.
//!
//! A web-native terminal multiplexer built with Rust, tokio, and axum.
//!
//! ## Features
//!
//! - WebSocket-based communication
//! - Windows PTY support via portable-pty
//! - Multiple terminal panes per session
//! - Token-based authentication
//!
//! ## Protocol
//!
//! Clients send JSON messages with an `action` field, and servers respond
//! with JSON messages with an `event` field.

pub mod auth;
pub mod messages;
pub mod pty_manager;
pub mod state;
pub mod ws_handler;

// Re-export commonly used types
pub use auth::{validate_token, AuthResult};
pub use messages::{ClientMessage, ServerMessage, PaneInfo, PaneGroupInfo};
pub use state::{AppState, Pane, PaneGroup};
pub use pty_manager::{default_shell_program, resolve_shell_program, PtyManager};
pub use ws_handler::{create_router, ws_handler, health_handler};
