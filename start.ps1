# Terminal Multiplexer Start Script
# Generates auth token, starts cloudflared tunnel, and runs the backend

$backendDir = $PSScriptRoot

# Generate a random 6-character alphanumeric token
$env:AUTH_TOKEN = -join ((97..122) + (48..57) | Get-Random -Count 6 | ForEach-Object {[char]$_})
$token = $env:AUTH_TOKEN
Write-Host "Auth Token: $token" -ForegroundColor Green

# Check if cloudflared.exe exists locally
$cloudflaredExe = "$backendDir\cloudflared.exe"
if (-not (Test-Path $cloudflaredExe)) {
    Write-Host "cloudflared not found at $cloudflaredExe" -ForegroundColor Red
    Write-Host "Download from: https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/install-and-setup/tunnel-guide/local/" -ForegroundColor Yellow
    exit 1
}

# Temp log file
$tunnelLog = "$env:TEMP\termote_tunnel.log"

# 1. Kill any dangling backend processes holding port 9090
Write-Host "Cleaning up any stale processes..."
Stop-Process -Name "termote" -Force -ErrorAction SilentlyContinue
Stop-Process -Name "cloudflared" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

Write-Host "Starting Cloudflare Tunnel..."

# 2. Start cloudflared and capture stderr (where the URL is printed)
$process = Start-Process -FilePath $cloudflaredExe -ArgumentList "tunnel", "--url", "http://localhost:9090" -NoNewWindow -PassThru -RedirectStandardError $tunnelLog

# 3. Wait up to 15 seconds for the tunnel to initialize and write the URL
Write-Host "Waiting for tunnel URL..." -ForegroundColor Yellow
$cloudflareUrl = ""
$startTime = Get-Date
while (((Get-Date) - $startTime).TotalSeconds -lt 15) {
    if (Test-Path $tunnelLog) {
        $content = Get-Content $tunnelLog -ErrorAction SilentlyContinue
        foreach ($line in $content) {
            if ($line -match '(https://[a-zA-Z0-9_-]+\.trycloudflare\.com)') {
                $cloudflareUrl = $Matches[1]
                break
            }
        }
    }
    if ($cloudflareUrl) { break }
    Start-Sleep -Seconds 1
}

if ($cloudflareUrl) {
    Write-Host "Cloudflare Tunnel URL: $cloudflareUrl" -ForegroundColor Cyan
    $wsUrl = $cloudflareUrl -replace '^https://', 'wss://'
    $env:TUNNEL_URL = $wsUrl
    Write-Host "WebSocket endpoint: $wsUrl/ws" -ForegroundColor Cyan
    # Write to .env so backend picks it up
    Set-Content -Path "$backendDir\.env" -Value "AUTH_TOKEN=$token`nTUNNEL_URL=$wsUrl" -Encoding UTF8
} else {
    Write-Host "Warning: Could not determine tunnel URL" -ForegroundColor Yellow
    if (Test-Path $tunnelLog) {
        Write-Host "Log contents:" -ForegroundColor Yellow
        Get-Content $tunnelLog
    }
}

# Run compiled Rust server in background
Set-Location $backendDir
Start-Process -FilePath "$backendDir\target\release\termote.exe" -WindowStyle Hidden

# Cleanup (only runs if termote exits)
if ($process -and -not $process.HasExited) {
    Stop-Process $process.Id -Force -ErrorAction SilentlyContinue
}
Remove-Item $tunnelLog -ErrorAction SilentlyContinue
