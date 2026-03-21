//! Terminal multiplexer backend entry point.
//!
//! A web-native terminal multiplexer server that handles WebSocket connections,
//! PTY spawning, and terminal I/O.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::serve;
use tower_http::cors::{CorsLayer, Any};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use termux_multiplexer::{create_router, AppState};

#[tokio::main]
async fn main() {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(true)
        .with_thread_ids(true)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("Starting Terminal Multiplexer Backend");

    // Load environment variables from .env file if present
    dotenvy::dotenv().ok();

    // Get auth token from environment
    let auth_token = std::env::var("AUTH_TOKEN")
        .unwrap_or_else(|_| {
            info!("AUTH_TOKEN not set in environment, generating a random one");
            generate_token()
        });

    info!("Auth token configured");

    // Create application state
    let state = Arc::new(AppState::new(auth_token));

    // Create router with CORS
    let app = create_router(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Configure listen address
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    info!("Listening on {}", addr);
    info!("WebSocket endpoint: ws://{}/ws", addr);
    info!("Health check: http://{}/health", addr);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await
        .expect("Failed to bind to address");

    serve(listener, app)
        .await
        .expect("Server error");
}

/// Generates a random 6-character alphanumeric token.
fn generate_token() -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    // Simple deterministic token based on current time
    // In production, use the rand crate for proper randomness
    let mut token = String::with_capacity(6);
    let mut seed = now;
    for _ in 0..6 {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let idx = (seed >> 16) as usize % CHARSET.len();
        token.push(CHARSET[idx] as char);
    }
    token
}
