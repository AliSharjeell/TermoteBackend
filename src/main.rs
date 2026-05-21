//! Termote backend entry point.
//!
//! A web-native terminal multiplexer server that handles WebSocket connections,
//! PTY spawning, and terminal I/O.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::env;

use axum::serve;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use tracing::{info, Level, error};
use tracing_subscriber::{fmt, layer::SubscriberExt, EnvFilter};

use termote::{create_router, default_shell_program, AppState};

fn arg_value(name: &str) -> Option<String> {
    let flag = format!("--{}", name);
    let prefix = format!("{}=", flag);
    let args: Vec<String> = env::args().collect();

    args.iter()
        .position(|arg| arg == &flag)
        .and_then(|idx| args.get(idx + 1).cloned())
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix(&prefix).map(|value| value.to_string()))
        })
}

fn build_launch_url(frontend_url: &str, tunnel_url: &str, auth_token: &str) -> String {
    let encoded_tunnel = urlencoding::encode(tunnel_url);
    let encoded_token = urlencoding::encode(auth_token);
    let path = format!("/dashboard/?tunnel={}&token={}", encoded_tunnel, encoded_token);

    if frontend_url.trim().is_empty() || frontend_url == "/" {
        path
    } else {
        format!("{}{}", frontend_url.trim_end_matches('/'), path)
    }
}

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

                            if line == "ban-list" {
                                // List all banned IPs
                                let banned = state.get_banned_ips().await;
                                let response = if banned.is_empty() {
                                    "No banned IPs".to_string()
                                } else {
                                    banned.join("\n")
                                };
                                let _ = stream.write_all(response.as_bytes()).await;
                                let _ = stream.write_all(b"\n").await;
                            } else if let Some(ip) = line.strip_prefix("ban:") {
                                // Ban an IP address
                                let ip = ip.trim();
                                if state.is_ip_banned(ip).await {
                                    let _ = stream.write_all(b"IP already banned\n").await;
                                } else {
                                    state.ban_ip(ip).await;
                                    let _ = stream.write_all(b"IP banned successfully\n").await;
                                }
                            } else if let Some(ip) = line.strip_prefix("unban:") {
                                // Unban an IP address
                                let ip = ip.trim();
                                if state.unban_ip(ip).await {
                                    let _ = stream.write_all(b"IP unbanned successfully\n").await;
                                } else {
                                    let _ = stream.write_all(b"IP was not banned\n").await;
                                }
                            } else if let Some(path) = line.strip_prefix("open_dir:") {
                                let path = path.trim();
                                let shell = default_shell_program();
                                if let Err(e) = state.spawn_pane_at_dir(path, &shell).await {
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
    let port = arg_value("port")
        .or_else(|| std::env::var("PORT").ok())
        .and_then(|p| p.parse().ok())
        .unwrap_or(9090);

    // Optional absolute frontend URL. Empty means same-origin static UI.
    let frontend_url = arg_value("public-base-url")
        .or_else(|| std::env::var("PUBLIC_BASE_URL").ok())
        .or_else(|| std::env::var("FRONTEND_URL").ok())
        .unwrap_or_default();

    // Get tunnel URL (public URL of this server for WebSocket connections)
    let tunnel_url = arg_value("tunnel-url")
        .or_else(|| std::env::var("TUNNEL_URL").ok())
        .unwrap_or_else(|| {
            // Default to localhost - user should configure this for production
            info!("TUNNEL_URL not set, defaulting to localhost (configure for production!)");
            format!("ws://127.0.0.1:{}", port)
        });

    let frontend_dir = arg_value("frontend-dir")
        .or_else(|| std::env::var("FRONTEND_DIR").ok())
        .map(PathBuf::from);

    info!("Frontend URL: {}", if frontend_url.is_empty() { "(same-origin)" } else { &frontend_url });
    if let Some(ref dir) = frontend_dir {
        info!("Serving frontend from: {}", dir.display());
    } else {
        info!("No frontend directory configured; API/WebSocket routes only");
    }
    info!("Tunnel URL: {}", tunnel_url);

    // Parse CLI arguments for cold start initial directory
    let cold_start_dir: Option<String> = arg_value("initial-dir");

    if let Some(ref dir) = cold_start_dir {
        info!("Cold start with initial directory: {}", dir);
    }

    // Create application state
    let state = Arc::new(AppState::new(
        auth_token.clone(),
        frontend_url.clone(),
        tunnel_url.clone(),
        cold_start_dir,
    ).await);

    // Start IPC server for single-instance behavior
    let ipc_state = state.clone();
    tokio::spawn(async move {
        run_ipc_server(ipc_state).await;
    });

    // Create router with CORS
    let app = create_router(state.clone(), frontend_dir.clone())
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
    let launch_url = build_launch_url(&frontend_url, &tunnel_url, &auth_token);

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
        let shell = default_shell_program();
        if let Err(e) = state.spawn_pane_at_dir(dir, &shell).await {
            error!("Failed to spawn initial pane at {}: {}", dir, e);
        }
    }

    serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
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
