//! Authentication handling for the terminal multiplexer.
//!
//! Validates tokens against the configured auth token.

use crate::state::AppState;

/// Validates an authentication token against the app state.
///
/// Returns true if the token is valid.
pub async fn validate_token(state: &AppState, token: &str) -> bool {
    state.validate_token(token).await
}

/// Authentication result with optional message.
#[derive(Debug)]
pub struct AuthResult {
    pub success: bool,
    pub message: Option<String>,
}

impl AuthResult {
    /// Creates a successful auth result.
    pub fn success() -> Self {
        Self {
            success: true,
            message: Some("Authentication successful".to_string()),
        }
    }

    /// Creates a failed auth result with a message.
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: Some(message.into()),
        }
    }
}
