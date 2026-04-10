# Terminal Multiplexer Start Script
# Generates auth token, starts Microsoft Dev Tunnel, and runs the backend

Write-Host ""
Write-Host "  TERMOTE" -ForegroundColor Cyan
Write-Host "  =======" -ForegroundColor Cyan
Write-Host "  Turn any browser into a full-powered, multi-pane terminal for your PC." -ForegroundColor Gray
Write-Host ""

$backendDir = $PSScriptRoot

# Generate a random 6-character alphanumeric token
$env:AUTH_TOKEN = -join ((97..122) + (48..57) | Get-Random -Count 6 | ForEach-Object {[char]$_})
$token = $env:AUTH_TOKEN

# 1. Kill dangling processes
Write-Host "Cleaning up any stale processes..." -ForegroundColor DarkGray
Stop-Process -Name "termote", "devtunnel", "ssh" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 1500

# 2. Clear old logs
$tunnelLog    = "$env:TEMP\termote_tunnel.log"
$tunnelErrLog = "$env:TEMP\termote_tunnel_err.log"
Remove-Item $tunnelLog, $tunnelErrLog -Force -ErrorAction SilentlyContinue

# 3. Find devtunnel executable
$devtunnelExe = "$env:USERPROFILE\termote\bin\devtunnel.exe"
if (-not (Test-Path $devtunnelExe)) {
    $devtunnelExe = "$backendDir\devtunnel.exe"
}
if (-not (Test-Path $devtunnelExe)) {
    $found = Get-Command devtunnel -ErrorAction SilentlyContinue
    if ($found) { $devtunnelExe = $found.Source }
    else {
        Write-Host "devtunnel not found! Run install.ps1 first." -ForegroundColor Red
        exit 1
    }
}

# Verify devtunnel works
Write-Host "Using devtunnel: $devtunnelExe" -ForegroundColor DarkGray
try {
    $versionOutput = & "$devtunnelExe" --version 2>&1 | Out-String
} catch {
    $versionOutput = ""
}
if ($versionOutput -notmatch 'version') {
    Write-Host "ERROR: $devtunnelExe is not Microsoft devtunnel (got: $versionOutput)" -ForegroundColor Red
    exit 1
}

# 4. Check auth and re-auth if expired
Write-Host "Checking Dev Tunnel authentication..." -ForegroundColor Yellow
$output = & "$devtunnelExe" user show 2>&1
if ($output -match "expired" -or $LASTEXITCODE -ne 0) {
    Write-Host "Re-authenticating with device code..."
    & "$devtunnelExe" user login -g
}

# 5. Start devtunnel
Write-Host "Starting Microsoft Dev Tunnel..." -ForegroundColor Yellow
$process = Start-Process -FilePath "$devtunnelExe" `
    -ArgumentList "host", "-p", "9090", "--allow-anonymous" `
    -NoNewWindow -PassThru `
    -RedirectStandardOutput $tunnelLog `
    -RedirectStandardError $tunnelErrLog

# 6. Wait for the tunnel URL
Write-Host "Waiting for tunnel URL..." -ForegroundColor DarkGray
$devtunnelUrl = ""
$startTime = Get-Date

while (((Get-Date) - $startTime).TotalSeconds -lt 20) {
    foreach ($logFile in @($tunnelLog, $tunnelErrLog)) {
        if (Test-Path $logFile) {
            foreach ($line in (Get-Content $logFile -ErrorAction SilentlyContinue)) {
                # Look specifically for the "Connect via browser:" line
                if ($line -match 'Connect via browser:\s*(https://[^\s]+)') {
                    $devtunnelUrl = $Matches[1]
                    break
                }
            }
        }
    }
    if ($devtunnelUrl) { break }
    if ($process.HasExited) {
        Write-Host "ERROR: devtunnel exited early. Check login with: devtunnel user show" -ForegroundColor Red
        Get-Content $tunnelErrLog -ErrorAction SilentlyContinue
        exit 1
    }
    Start-Sleep -Seconds 1
}

# 7. Build WSS URL and write .env
$wsUrl = $devtunnelUrl -replace '^https://', 'wss://'
$env:TUNNEL_URL = $wsUrl
Set-Content -Path "$backendDir\.env" -Value "AUTH_TOKEN=$token`nTUNNEL_URL=$wsUrl" -Encoding ASCII

# 8. Start Rust backend
$backendExe = "$backendDir\target\release\termote.exe"
if (-not (Test-Path $backendExe)) {
    Write-Host "ERROR: termote.exe not found. Run install.ps1 first to build the backend." -ForegroundColor Red
    exit 1
}
Set-Location $backendDir

# Pass initial directory for cold start auto-spawn
$backendArgs = @()
if ($env:TERMOTE_INITIAL_DIR) {
    Write-Host "Cold start initial directory: $env:TERMOTE_INITIAL_DIR" -ForegroundColor DarkGray
    $backendArgs += "--initial-dir"
    $backendArgs += $env:TERMOTE_INITIAL_DIR
}

if ($backendArgs.Count -gt 0) {
    Start-Process -FilePath $backendExe -ArgumentList $backendArgs -WorkingDirectory $backendDir -WindowStyle Hidden
} else {
    Start-Process -FilePath $backendExe -WorkingDirectory $backendDir -WindowStyle Hidden
}

# 9. Print launch URL
Write-Host ""
Write-Host "========================================================" -ForegroundColor Green
Write-Host " Termote is Live via Microsoft Dev Tunnels!" -ForegroundColor White
Write-Host " URL: https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($wsUrl + '/ws'))&token=$token" -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Green
Write-Host ""