//! Termote backend entry point.
//!
//! A web-native terminal multiplexer server that handles WebSocket connections,
//! PTY spawning, and terminal I/O.

use std::net::SocketAddr;
use std::sync::Arc;
use std::env;

use axum::serve;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use tracing::{info, Level, error};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

use termote::{create_router, AppState};

/// IPC server for single-instance behavior.
/// Listens on localhost:9091 for commands like "open_dir:<path>"
async fn run_ipc_server(state: Arc<AppState>) {
    let addr = "127.0.0.1:9091";
    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind IPC server on {}: {}", addr, e);
            return;
        }
    };
    info!("IPC server listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    let mut reader = BufReader::new(&mut stream);
                    let mut line = String::new();
                    if let Ok(n) = reader.read_line(&mut line).await {
                        if n > 0 {
                            let line = line.trim();
                            info!("IPC received: {}", line);
                            if let Some(path) = line.strip_prefix("open_dir:") {
                                let path = path.trim();
                                if let Err(e) = state.spawn_pane_at_dir(path, "powershell.exe").await {
                                    error!("Failed to spawn pane at {}: {}", path, e);
                                }
                            }
                        }
                    }
                });
            }
            Err(e) => {
                error!("IPC accept error: {}", e);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize logging to file in TEMP directory
    let log_dir = std::env::temp_dir();
    let log_file = log_dir.join("termote.log");

    let file_appender = tracing_appender::rolling::daily(log_dir, "termote.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so it lives for the entire program duration
    // This is necessary because the guard must outlive the tracing subscriber
    std::mem::forget(guard);

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true);

    let stdout_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(false);

    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env().add_directive(Level::DEBUG.into()))
        .with(file_layer)
        .with(stdout_layer);
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("Starting Terminal Multiplexer Backend");
    info!("Log file: {}", log_file.display());

    // Load environment variables from .env file if present
    dotenvy::dotenv().ok();

    // Get auth token from environment
    let auth_token = std::env::var("AUTH_TOKEN")
        .unwrap_or_else(|_| {
            info!("AUTH_TOKEN not set in environment, generating a random one");
            generate_token()
        });

    info!("Auth token configured");

    // Configure listen address
    let port = std::env::var("PORT").map(|p| p.parse().unwrap_or(9090)).unwrap_or(9090);

    // Get frontend URL (where the React app is hosted)
    let frontend_url = std::env::var("FRONTEND_URL")
        .unwrap_or_else(|_| "https://termote.vercel.app".to_string());

    // Get tunnel URL (public URL of this server for WebSocket connections)
    let tunnel_url = std::env::var("TUNNEL_URL")
        .unwrap_or_else(|_| {
            // Default to localhost - user should configure this for production
            info!("TUNNEL_URL not set, defaulting to localhost (configure for production!)");
            format!("ws://127.0.0.1:{}", port)
        });

    info!("Frontend URL: {}", frontend_url);
    info!("Tunnel URL: {}", tunnel_url);

    // Parse CLI arguments for cold start initial directory
    let cold_start_dir: Option<String> = env::args()
        .skip(1) // Skip program name
        .collect::<Vec<_>>()
        .iter()
        .position(|arg| arg == "--initial-dir")
        .and_then(|idx| env::args().skip(idx + 1).next())
        .or_else(|| {
            // Also check --initial-dir=VALUE format
            env::args()
                .skip(1)
                .find(|arg| arg.starts_with("--initial-dir="))
                .map(|arg| arg.trim_start_matches("--initial-dir=").to_string())
        });

    if let Some(ref dir) = cold_start_dir {
        info!("Cold start with initial directory: {}", dir);
    }

    // Create application state
    let state = Arc::new(AppState::new(
        auth_token.clone(),
        frontend_url.clone(),
        tunnel_url.clone(),
        cold_start_dir,
    ));

    // Start IPC server for single-instance behavior
    let ipc_state = state.clone();
    tokio::spawn(async move {
        run_ipc_server(ipc_state).await;
    });

    // Create router with CORS
    let app = create_router(state.clone())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );

    // Configure listen address
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    info!("Listening on {}", addr);
    info!("WebSocket endpoint: ws://{}/ws", addr);
    info!("Health check: http://{}/health", addr);
    info!("Launch (auto-connect): http://{}/launch", addr);

    // Build the launch URL
    let encoded_tunnel = urlencoding::encode(&tunnel_url);
    let encoded_token = urlencoding::encode(&auth_token);
    let launch_url = format!("{}/?tunnel={}&token={}", frontend_url, encoded_tunnel, encoded_token);

    // Don't auto-open browser — the termote shim handles opening the browser
    // when connecting to an existing session. Fresh starts show the URL in terminal.
    info!("Launch URL (for manual open): {}", launch_url);

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await
        .expect("Failed to bind to address");

    // After server starts, check if this is a cold start with an initial directory
    // and auto-spawn the first terminal
    if let Some(ref dir) = state.cold_start_dir {
        info!("Cold start detected, spawning initial terminal at: {}", dir);
        if let Err(e) = state.spawn_pane_at_dir(dir, "powershell.exe").await {
            error!("Failed to spawn initial pane at {}: {}", dir, e);
        }
    }

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
