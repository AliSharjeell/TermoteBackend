# Terminal Multiplexer Start Script
# Generates auth token, starts cloudflared tunnel, and runs the backend

# Generate a random 6-character alphanumeric token
$env:AUTH_TOKEN = -join ((97..122) + (48..57) | Get-Random -Count 6 | ForEach-Object {[char]$_})
$env:AUTH_TOKEN | Out-File -FilePath ".env"

$token = $env:AUTH_TOKEN
Write-Host "Auth Token: $token" -ForegroundColor Green
Write-Host "Starting Rust backend..."

# Check if cloudflared is installed
$cloudflaredPath = Get-Command cloudflared -ErrorAction SilentlyContinue
if (-not $cloudflaredPath) {
    Write-Host "cloudflared not found. Please install cloudflared first." -ForegroundColor Red
    Write-Host "Download from: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/tunnel-guide/local/" -ForegroundColor Yellow
    exit 1
}

# Start cloudflared tunnel
Write-Host "Starting Cloudflare Tunnel..."
$cloudflared = Start-Process -FilePath "cloudflared" -ArgumentList "tunnel", "--url", "http://localhost:9090" -NoNewWindow -PassThru -RedirectStandardOutput "tunnel.log"

Start-Sleep 5

# Parse the tunnel URL from the log
$cloudflareUrl = ""
try {
    $logContent = Get-Content "tunnel.log" -Raw
    if ($logContent -match 'try routing through our network.*"(https://[^"]+)"') {
        $cloudflareUrl = $Matches[1]
    } elseif ($logContent -match 'Your quick Tunnel has been created!.*?(https://[^"\s]+)') {
        $cloudflareUrl = $Matches[1]
    }
} catch {
    Write-Host "Warning: Could not parse tunnel URL from log" -ForegroundColor Yellow
}

if ($cloudflareUrl) {
    Write-Host "Cloudflare Tunnel URL: $cloudflareUrl" -ForegroundColor Cyan
    Write-Host "WebSocket endpoint: $cloudflareUrl/ws" -ForegroundColor Cyan
} else {
    Write-Host "Warning: Could not determine tunnel URL" -ForegroundColor Yellow
    Get-Content "tunnel.log" | Select-Object -First 10
}

# Run Rust server
Set-Location $PSScriptRoot
cargo run --release
