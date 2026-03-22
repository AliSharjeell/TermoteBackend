# Terminal Multiplexer Start Script
# Generates auth token, starts Microsoft Dev Tunnel, and runs the backend

$backendDir = $PSScriptRoot

# Generate a random 6-character alphanumeric token
$env:AUTH_TOKEN = -join ((97..122) + (48..57) | Get-Random -Count 6 | ForEach-Object {[char]$_})
$token = $env:AUTH_TOKEN

# 1. Clear old logs
$tunnelLog = "$env:TEMP\termote_tunnel.log"
if (Test-Path $tunnelLog) { Remove-Item $tunnelLog -Force }

# 2. Kill dangling processes
Write-Host "Cleaning up any stale processes..." -ForegroundColor DarkGray
Stop-Process -Name "termote", "devtunnel" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# Check if devtunnel.exe exists
$devtunnelExe = "$backendDir\devtunnel.exe"
if (-not (Test-Path $devtunnelExe)) {
    $globalDevtunnel = Get-Command devtunnel -ErrorAction SilentlyContinue
    if ($globalDevtunnel) {
        $devtunnelExe = $globalDevtunnel.Source
    } else {
        Write-Host "devtunnel not found. Run install.ps1 first to install it." -ForegroundColor Red
        exit 1
    }
}

Write-Host "Starting Microsoft Dev Tunnel..." -ForegroundColor Yellow

# 3. Start devtunnel host
# --allow-anonymous: anyone with the URL can connect (no auth needed)
$process = Start-Process -FilePath $devtunnelExe -ArgumentList "host", "-p", "9090", "--allow-anonymous" -NoNewWindow -PassThru -RedirectStandardOutput $tunnelLog

# 4. Wait up to 15 seconds for the URL
Write-Host "Waiting for tunnel URL..." -ForegroundColor DarkGray
$devtunnelUrl = ""
$startTime = Get-Date

while (((Get-Date) - $startTime).TotalSeconds -lt 15) {
    if (Test-Path $tunnelLog) {
        $content = Get-Content $tunnelLog -ErrorAction SilentlyContinue
        foreach ($line in $content) {
            # Dev Tunnels URLs look like: https://abc123-4567890.devtunnel.io
            if ($line -match '(https://[a-zA-Z0-9_-]+\.devtunnel\.io)') {
                $devtunnelUrl = $Matches[1]
                break
            }
        }
    }
    if ($devtunnelUrl) { break }
    if ($process.HasExited) {
        Write-Host "Dev tunnel process exited early with code: $($process.ExitCode)" -ForegroundColor Red
        break
    }
    Start-Sleep -Seconds 1
}

if ($devtunnelUrl) {
    $wsUrl = $devtunnelUrl -replace '^https://', 'wss://'
    $env:TUNNEL_URL = $wsUrl

    # Write to .env so backend picks it up
    Set-Content -Path "$backendDir\.env" -Value "AUTH_TOKEN=$token`nTUNNEL_URL=$wsUrl" -Encoding UTF8

    Write-Host ""
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host " Termote is Live!" -ForegroundColor White
    Write-Host " Tunnel URL: $devtunnelUrl" -ForegroundColor Cyan
    Write-Host " WebSocket: $wsUrl/ws" -ForegroundColor Cyan
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host ""
} else {
    Write-Host "Warning: Could not determine Dev Tunnel URL." -ForegroundColor Red
    Write-Host "--- Log Output ---" -ForegroundColor Yellow
    if (Test-Path $tunnelLog) { Get-Content $tunnelLog }
}

# 5. Run compiled Rust server in background
Set-Location $backendDir
Start-Process -FilePath "$backendDir\target\release\termote.exe" -WindowStyle Hidden
