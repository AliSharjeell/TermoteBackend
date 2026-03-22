# Terminal Multiplexer Start Script
# Generates auth token, starts Microsoft Dev Tunnel, and runs the backend

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

# Verify it's actually devtunnel before running it
$versionOutput = (& $devtunnelExe --version 2>&1) -join ' '
if ($versionOutput -notmatch 'version') {
    Write-Host "ERROR: $devtunnelExe is not Microsoft devtunnel (got: $versionOutput)" -ForegroundColor Red
    exit 1
}

# 4. Start devtunnel
Write-Host "Starting Microsoft Dev Tunnel..." -ForegroundColor Yellow
$process = Start-Process -FilePath $devtunnelExe `
    -ArgumentList "host", "-p", "9090", "--allow-anonymous" `
    -NoNewWindow -PassThru `
    -RedirectStandardOutput $tunnelLog `
    -RedirectStandardError $tunnelErrLog

# 5. Wait for the tunnel URL
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

# 6. Build WSS URL and write .env
$wsUrl = $devtunnelUrl -replace '^https://', 'wss://'
$env:TUNNEL_URL = $wsUrl
Set-Content -Path "$backendDir\.env" -Value "AUTH_TOKEN=$token`nTUNNEL_URL=$wsUrl" -Encoding UTF8

# 7. Start Rust backend
Set-Location $backendDir
Start-Process -FilePath "$backendDir\target\release\termote.exe" -WindowStyle Hidden

# 8. Print launch URL
Write-Host ""
Write-Host "========================================================" -ForegroundColor Green
Write-Host " Termote is Live via Microsoft Dev Tunnels!" -ForegroundColor White
Write-Host " URL: https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($wsUrl + '/ws'))&token=$token" -ForegroundColor Cyan
Write-Host "========================================================" -ForegroundColor Green
Write-Host ""