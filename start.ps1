# Terminal Multiplexer Start Script
# Generates auth token, starts Serveo SSH tunnel, and runs the backend

$backendDir = $PSScriptRoot

# Generate a random 6-character alphanumeric token
$env:AUTH_TOKEN = -join ((97..122) + (48..57) | Get-Random -Count 6 | ForEach-Object {[char]$_})
$token = $env:AUTH_TOKEN

# 1. Clear old logs
$tunnelLog = "$env:TEMP\termote_tunnel.log"
if (Test-Path $tunnelLog) { Remove-Item $tunnelLog -Force }

# 2. Kill dangling processes
Write-Host "Cleaning up any stale processes..." -ForegroundColor DarkGray
Stop-Process -Name "termote", "ssh" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

Write-Host "Starting Serveo.net SSH tunnel..." -ForegroundColor Yellow

# 3. The exact command you ran manually, minus the invisible flags!
$sshCmd = "ssh -R 80:localhost:9090 serveo.net -o StrictHostKeyChecking=no -o ServerAliveInterval=60"
Start-Process -FilePath "cmd.exe" -ArgumentList "/c `"$sshCmd > `"$tunnelLog`" 2>&1`"" -WindowStyle Hidden

# 4. Wait up to 15 seconds for the URL
Write-Host "Waiting for tunnel URL..." -ForegroundColor DarkGray
$serveoUrl = ""
$startTime = Get-Date

while (((Get-Date) - $startTime).TotalSeconds -lt 15) {
    if (Test-Path $tunnelLog) {
        $content = Get-Content $tunnelLog -ErrorAction SilentlyContinue
        foreach ($line in $content) {
            # Matches your exact output: serveousercontent.com
            if ($line -match '(https?://[a-zA-Z0-9_-]+\.serveousercontent\.com)') {
                $serveoUrl = $Matches[1]
                break
            }
        }
    }
    if ($serveoUrl) { break }
    Start-Sleep -Seconds 1
}

if ($serveoUrl) {
    $wsUrl = $serveoUrl -replace '^http://', 'ws://' -replace '^https://', 'wss://'
    $env:TUNNEL_URL = $wsUrl
    
    # Write to .env so backend picks it up
    Set-Content -Path "$backendDir\.env" -Value "AUTH_TOKEN=$token`nTUNNEL_URL=$wsUrl" -Encoding UTF8
    
    Write-Host ""
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host " Termote is Live!" -ForegroundColor White
    Write-Host " Terminal URL: https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($wsUrl + '/ws'))&token=$token" -ForegroundColor Cyan
    Write-Host "========================================================" -ForegroundColor Green
    Write-Host ""
} else {
    Write-Host "Warning: Could not determine Serveo tunnel URL." -ForegroundColor Red
    Write-Host "--- Log Output ---" -ForegroundColor Yellow
    if (Test-Path $tunnelLog) { Get-Content $tunnelLog }
}

# 5. Run compiled Rust server in background
Set-Location $backendDir
Start-Process -FilePath "$backendDir\target\release\termote.exe" -WindowStyle Hidden