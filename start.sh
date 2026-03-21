#!/bin/bash
# Terminal Multiplexer Start Script
# Generates auth token, starts cloudflared tunnel, and runs the backend

set -e

# Generate a random 6-character alphanumeric token
AUTH_TOKEN=$(head -c 100 /dev/urandom | tr -dc 'a-z0-9' | head -c 6)
export AUTH_TOKEN="$AUTH_TOKEN"
echo "$AUTH_TOKEN" > .env

echo "Auth Token: $AUTH_TOKEN"
echo "Starting Rust backend..."

# Check if cloudflared is installed
if ! command -v cloudflared &> /dev/null; then
    echo "cloudflared not found. Please install cloudflared first."
    echo "Download from: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/tunnel-guide/local/"
    exit 1
fi

# Start cloudflared tunnel
echo "Starting Cloudflare Tunnel..."
cloudflared tunnel --url http://localhost:9090 > tunnel.log 2>&1 &
CLOUDFLARED_PID=$!

sleep 5

# Parse the tunnel URL from the log
if grep -q "try routing through our network" tunnel.log; then
    CLOUDFLARE_URL=$(grep -o 'try routing through our network.*"[^"]*"' tunnel.log | grep -o '"https://[^"]*"' | tr -d '"')
elif grep -q "Your quick Tunnel has been created" tunnel.log; then
    CLOUDFLARE_URL=$(grep -o 'https://[^"]*\.trycloudflare\.com' tunnel.log | head -1)
else
    echo "Warning: Could not determine tunnel URL"
    cat tunnel.log | head -10
fi

if [ -n "$CLOUDFLARE_URL" ]; then
    echo "Cloudflare Tunnel URL: $CLOUDFLARE_URL"
    echo "WebSocket endpoint: $CLOUDFLARE_URL/ws"
fi

# Cleanup function
cleanup() {
    echo "Shutting down..."
    kill $CLOUDFLARED_PID 2>/dev/null || true
}
trap cleanup EXIT INT TERM

# Run Rust server
cd "$(dirname "$0")"
cargo run --release
