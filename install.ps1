param(
    [string]$RepoUrl = "https://github.com/AliSharjeell/Termote.git",
    [string]$Branch = "master"
)

$ErrorActionPreference = "Continue"

Write-Host ""
Write-Host "  ████████╗███████╗██████╗ ███╗   ███╗██████╗ ████████╗███████╗" -ForegroundColor Cyan
Write-Host "  ╚══██╔══╝██╔════╝██╔══██╗████╗ ████║██╔═══██╗╚══██╔══╝██╔════╝" -ForegroundColor Cyan
Write-Host "     ██║   █████╗  ██████╔╝██╔████╔██║██║   ██║   ██║   █████╗  " -ForegroundColor Cyan
Write-Host "     ██║   ██╔══╝  ██╔══██╗██║╚██╔╝██║██║   ██║   ██║   ██╔══╝  " -ForegroundColor Cyan
Write-Host "     ██║   ███████╗██║  ██║██║ ╚═╝ ██║╚██████╔╝   ██║   ███████╗" -ForegroundColor Cyan
Write-Host "     ╚═╝   ╚══════╝╚═╝  ╚═╝╚═╝     ╚═╝ ╚═════╝    ╚═╝   ╚══════╝" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Remote multiplexer pane terminal accessible from any browser" -ForegroundColor Gray
Write-Host ""

$installDir = "$env:USERPROFILE\termote"
$backendDir = "$installDir\backend"
$shimDir = "$env:USERPROFILE\.termote-bin"

# 1. Clone or update the repo
if (-not (Test-Path $installDir)) {
    Write-Host "[1/8] Cloning Termote repository..." -ForegroundColor Yellow
    git clone --depth 1 $RepoUrl $installDir
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to clone repository. Is Git installed?" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[1/8] Updating existing Termote installation..." -ForegroundColor Yellow
    Set-Location $installDir
    git pull origin $Branch

    # Sync updated scripts and binary from repo to installed location
    # $PSScriptRoot is the repo on disk (has our edits), $backendDir is the target install
   Write-Host "  Syncing updated files to installed location..." -ForegroundColor Gray
    Copy-Item -Path "$PSScriptRoot\start.ps1" -Destination "$backendDir\start.ps1" -Force
    Copy-Item -Path "$PSScriptRoot\install.ps1" -Destination "$backendDir\install.ps1" -Force
    # Delete old root-level stale files
    Remove-Item "$installDir\start.ps1" -Force -ErrorAction SilentlyContinue
    Remove-Item "$installDir\install.ps1" -Force -ErrorAction SilentlyContinue
}

# 2. Install Rust if not present
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[2/8] Installing Rust (first-time only, ~2 min)..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile "$env:TEMP\rustup-init.exe"
    & "$env:TEMP\rustup-init.exe" -y -q --default-toolchain stable
    Remove-Item "$env:TEMP\rustup-init.exe" -Force -ErrorAction SilentlyContinue
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")
    Write-Host "  Rust installed successfully!" -ForegroundColor Green
} else {
    Write-Host "[2/8] Rust already installed, skipping..." -ForegroundColor Gray
}

# 3. Installing Dev Tunnels with a sanity check
Write-Host "[3/8] Installing Microsoft Dev Tunnels CLI..." -ForegroundColor Yellow
$devtunnelPath = "$installDir\bin\devtunnel.exe"

# Ensure bin directory exists
if (-not (Test-Path "$installDir\bin")) {
    New-Item -Type Directory -Force "$installDir\bin" | Out-Null
}

# Use curl.exe (built into Windows 10/11) — handles GitHub redirects reliably
Write-Host "  Downloading devtunnel.exe..." -ForegroundColor Gray
curl.exe -L --silent --show-error -o $devtunnelPath "https://github.com/microsoft/dev-tunnels/releases/latest/download/devtunnel-win-x64.exe"

# Sanity Check: If the file is smaller than 1MB, it's definitely a failed download
if (-not (Test-Path $devtunnelPath) -or (Get-Item $devtunnelPath).Length -lt 1MB) {
    Write-Host "  curl.exe failed, trying direct MS download link..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://aka.ms/TunnelsCliDownload/win-x64" -OutFile $devtunnelPath -UseBasicParsing
}

if (-not (Test-Path $devtunnelPath) -or (Get-Item $devtunnelPath).Length -lt 1MB) {
    Write-Host "ERROR: All download attempts failed." -ForegroundColor Red
    Write-Host "  Manual fix: Download devtunnel.exe from https://aka.ms/TunnelsCliDownload/win-x64" -ForegroundColor Yellow
    Write-Host "  Save it to: $devtunnelPath" -ForegroundColor Yellow
    exit 1
}
Write-Host "  Dev Tunnels CLI installed and verified!" -ForegroundColor Green
# 4. Login to Microsoft Dev Tunnels (Official CLI Method)
Write-Host "[4/8] Microsoft Dev Tunnels login..." -ForegroundColor Yellow
Write-Host "  A browser window will now open for authentication." -ForegroundColor Cyan
Write-Host "  If the browser doesn't open, copy the link printed below." -ForegroundColor Gray
Write-Host ""

# Call the CLI directly to handle the login flow
& $devtunnelPath user login -g

if ($LASTEXITCODE -ne 0) {
    Write-Host "WARNING: Login was not completed or failed." -ForegroundColor Yellow
} else {
    Write-Host "  Login successful!" -ForegroundColor Green
}
# 4. Compile the Rust backend
Write-Host "[5/8] Compiling Rust backend..." -ForegroundColor Yellow
Set-Location $backendDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust compilation failed." -ForegroundColor Red
    exit 1
}
Write-Host "  Backend compiled successfully!" -ForegroundColor Green

# Kill running termote so we can overwrite the exe
Stop-Process -Name "termote" -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 500

# Copy freshly compiled binary to installed location
$targetDir = "$backendDir\target\release"
if (-not (Test-Path $targetDir)) {
    New-Item -Type Directory -Force $targetDir | Out-Null
}
Copy-Item -Path "$PSScriptRoot\target\release\termote.exe" `
          -Destination "$targetDir\termote.exe" -Force
Write-Host "  Binary synced to installed location." -ForegroundColor Green

# 5. Create shim directory and files
Write-Host "[6/8] Setting up termote commands..." -ForegroundColor Yellow

if (-not (Test-Path $shimDir)) {
    New-Item -Type Directory -Force $shimDir | Out-Null
}

$termoteShimContent = @'
# Smart termote launcher - VS Code style single instance
$backendDir = "$env:USERPROFILE\termote\backend"
$termoteDir = "$env:USERPROFILE\termote"

function Send-IpcCommand($cmd) {
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $client.Connect("127.0.0.1", 9091)
        $stream = $client.GetStream()
        $writer = New-Object System.IO.StreamWriter($stream)
        $writer.WriteLine($cmd)
        $writer.Flush()
        $stream.Close()
        $client.Close()
        return $true
    } catch { return $false }
}

$cwd = (Get-Location).Path

# 1. Check if the process exists AT ALL (prevents race conditions during boot)
$termoteProc = Get-Process -Name "termote" -ErrorAction SilentlyContinue

if ($termoteProc) {
    Write-Host "Termote is already running (or booting up). Waiting for it to be ready..." -ForegroundColor DarkGray

    # Wait up to 15 seconds for the boot to finish and port 9090 to open
    $isReady = $false
    for ($i = 0; $i -lt 15; $i++) {
        try {
            $resp = Invoke-WebRequest -Uri "http://127.0.0.1:9090/health" -UseBasicParsing -TimeoutSec 1 -EA SilentlyContinue
            if ($resp.StatusCode -eq 200) { $isReady = $true; break }
        } catch { }
        Start-Sleep -Seconds 1
    }

    if ($isReady) {
        Write-Host "Termote is ready. Sending open_dir IPC..." -ForegroundColor Cyan
        $sent = Send-IpcCommand "open_dir:$cwd"
        if ($sent) {
            Write-Host "New pane opened at: $cwd" -ForegroundColor Green
            exit 0
        } else {
            Write-Host "Failed to talk to existing Termote instance. It might be frozen." -ForegroundColor Red
        }
    } else {
        Write-Host "Existing Termote instance is hung and won't respond. Restarting it..." -ForegroundColor Yellow
        Stop-Process -Name "termote" -Force -EA SilentlyContinue
        Stop-Process -Name "devtunnel" -Force -EA SilentlyContinue
        Start-Sleep -Seconds 1
    }
}

# 2. If we get here, no instances are running. Start fresh!
Write-Host "Starting fresh Termote server..." -ForegroundColor Green
& "$termoteDir\start.ps1"
'@
Set-Content -Path "$shimDir\termote.ps1" -Value $termoteShimContent -Encoding UTF8

$cmdLines = @(
    "@echo off"
    "powershell -NoProfile -ExecutionPolicy Bypass -Command `"`& '%~dp0termote.ps1' %*`""
)
Set-Content -Path "$shimDir\termote.cmd" -Value $cmdLines -Encoding ASCII

$killLines = @(
    '# Termote Kill Script'
    'Write-Host "Stopping Termote..." -ForegroundColor Yellow'
    'Get-Process -Name "termote" -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue'
    'Get-Process -Name "devtunnel" -EA SilentlyContinue | Stop-Process -Force -EA SilentlyContinue'
    'Get-NetTCPConnection -LocalPort 9090 -EA SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -EA SilentlyContinue }'
    'Get-NetTCPConnection -LocalPort 9091 -EA SilentlyContinue | ForEach-Object { Stop-Process -Id $_.OwningProcess -Force -EA SilentlyContinue }'
    'Write-Host "All Termote instances stopped." -ForegroundColor Green'
)
Set-Content -Path "$shimDir\termote-kill.ps1" -Value $killLines -Encoding UTF8

$killCmdLines = @(
    "@echo off"
    "powershell -NoProfile -ExecutionPolicy Bypass -Command `"`& '%~dp0termote-kill.ps1'`""
)
Set-Content -Path "$shimDir\termote-kill.cmd" -Value $killCmdLines -Encoding ASCII

# termote-link: Display tunnel URL, password, and Ctrl+clickable share link
$linkLines = @(
    '# Termote Link Display'
    '$backendDir = "$env:USERPROFILE\termote\backend"'
    '$envFile = "$backendDir\.env"'
    ''
    'if (-not (Test-Path $envFile)) {'
    '    Write-Host "No .env file found. Is Termote running?" -ForegroundColor Red'
    '    exit 1'
    '}'
    ''
    '$content = Get-Content $envFile -Raw'
    '$tunnelUrl = if ($content -match ''TUNNEL_URL=(.+)'') { $Matches[1].Trim() } else { $null }'
    '$token = if ($content -match ''AUTH_TOKEN=(.+)'') { $Matches[1].Trim() } else { $null }'
    ''
    'if (-not $tunnelUrl -or -not $token) {'
    '    Write-Host "Tunnel URL or token not found in .env." -ForegroundColor Red'
    '    exit 1'
    '}'
    ''
    '$shareLink = "https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($tunnelUrl))&token=$([Uri]::EscapeDataString($token))"'
    '$wsLink = $tunnelUrl'
    ''
    '# OSC 8 hyperlink: ESC ] 8 ; ; URL ST'
    '# Format: `e]8;;URL`e\TEXT`e]8;;`e\'
    '$esc = "`e"'
    '$hyperlink = "$esc]8;;$shareLink$esc\$esc]8;;$esc\"'
    ''
    $launchUrl = "https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($wsUrl + '/ws'))&token=$token"
    $esc = [char]27
    $clickableLink = "${esc}]8;;${launchUrl}${esc}\Open Termote in Browser${esc}]8;;${esc}\"
    Write-Host ""
    Write-Host "  Termote Connection Info" -ForegroundColor Cyan
    Write-Host "  ─────────────────────" -ForegroundColor Cyan
    Write-Host "  Tunnel (WSS): " -NoNewline; Write-Host $wsUrl -ForegroundColor White
    Write-Host "  Password:     " -NoNewline; Write-Host $token -ForegroundColor White
    Write-Host "  Share Link:   " -NoNewline; Write-Host $clickableLink -ForegroundColor Green
    Write-Host ""
)
Set-Content -Path "$shimDir\termote-link.ps1" -Value $linkLines -Encoding UTF8

$linkCmdLines = @(
    "@echo off"
    "powershell -NoProfile -ExecutionPolicy Bypass -Command `"`& '%~dp0termote-link.ps1'`""
)
Set-Content -Path "$shimDir\termote-link.cmd" -Value $linkCmdLines -Encoding ASCII

# Add shimDir to PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$shimDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$shimDir", "User")
    $env:PATH += ";$shimDir"
    Write-Host "  Added to PATH." -ForegroundColor Green
}
Write-Host "  Commands installed." -ForegroundColor Green

# 6. Add "Open with Termote" context menu
Write-Host "[7/8] Adding Windows Explorer context menu..." -ForegroundColor Yellow

$termoteFileHandler = "$shimDir\termote-file.ps1"
$handlerLines = @(
    '$rawPath = $args[0]'
    'if (-not $rawPath) { $rawPath = (Get-Location).Path }'
    '# If it is a file, get the parent directory. If it is a directory, use it directly.'
    '$dir = if (Test-Path $rawPath -PathType Leaf) { Split-Path -Parent $rawPath } else { $rawPath }'
    '# Strip quotes just in case'
    '$dir = $dir -replace ''^"|"$'', '''''
    'Start-Process powershell -ArgumentList "-NoExit","-WindowStyle","Hidden","-Command","Set-Location -LiteralPath `"$dir`"; termote" '
)
Set-Content -Path $termoteFileHandler -Value $handlerLines -Encoding UTF8

# Folder background (right-click in empty space)
$regPath = "HKCU:\Software\Classes\Directory\Background\shell\Termote"
$cmdPath = "$regPath\command"
if (-not (Test-Path $regPath)) { New-Item -Path $regPath -Force | Out-Null }
Set-ItemProperty -Path $regPath -Name "(Default)" -Value "Open with Termote"
Set-ItemProperty -Path $regPath -Name "Icon" -Value "`"$shimDir\termote.ps1`",0"
if (-not (Test-Path $cmdPath)) { New-Item -Path $cmdPath -Force | Out-Null }
Set-ItemProperty -Path $cmdPath -Name "(Default)" -Value "powershell -WindowStyle Hidden -File `"$termoteFileHandler`" `"%V`""

# Folder icon (right-click on folder)
$regPath2 = "HKCU:\Software\Classes\Directory\shell\Termote"
$cmdPath2 = "$regPath2\command"
if (-not (Test-Path $regPath2)) { New-Item -Path $regPath2 -Force | Out-Null }
Set-ItemProperty -Path $regPath2 -Name "(Default)" -Value "Open with Termote"
Set-ItemProperty -Path $regPath2 -Name "Icon" -Value "`"$shimDir\termote.ps1`",0"
if (-not (Test-Path $cmdPath2)) { New-Item -Path $cmdPath2 -Force | Out-Null }
Set-ItemProperty -Path $cmdPath2 -Name "(Default)" -Value "powershell -WindowStyle Hidden -File `"$termoteFileHandler`" `"%1`""

Write-Host "  Context menu installed." -ForegroundColor Green

# 7. Start termote
Write-Host "[8/8] Starting Termote server..." -ForegroundColor Yellow

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Installation complete!" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Available commands:" -ForegroundColor White
Write-Host "  - termote         : Start or connect to Termote" -ForegroundColor Cyan
Write-Host "  - termote-kill   : Stop all Termote instances" -ForegroundColor Cyan
Write-Host "  - termote-link   : Show tunnel URL, password & share link" -ForegroundColor Cyan
Write-Host "  - Right-click in folder -> 'Open with Termote'" -ForegroundColor Cyan
Write-Host ""
Write-Host "  If commands not found in new terminal, run:" -ForegroundColor Yellow
Write-Host '    $env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")' -ForegroundColor Gray
Write-Host ""

# Was: & "$termoteDir\backend\start.ps1"
& "$installDir\backend\start.ps1"