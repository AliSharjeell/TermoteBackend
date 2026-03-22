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
    Write-Host "[1/7] Cloning Termote repository..." -ForegroundColor Yellow
    git clone --depth 1 $RepoUrl $installDir
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to clone repository. Is Git installed?" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[1/7] Updating existing Termote installation..." -ForegroundColor Yellow
    Set-Location $installDir
    git pull origin $Branch
}

# 2. Install Rust if not present
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[2/7] Installing Rust (first-time only, ~2 min)..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile "$env:TEMP\rustup-init.exe"
    & "$env:TEMP\rustup-init.exe" -y -q --default-toolchain stable
    Remove-Item "$env:TEMP\rustup-init.exe" -Force -ErrorAction SilentlyContinue
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")
    Write-Host "  Rust installed successfully!" -ForegroundColor Green
} else {
    Write-Host "[2/7] Rust already installed, skipping..." -ForegroundColor Gray
}

# 3. Install cloudflared (Winget with direct download fallback)
$cloudflaredInstalled = $false
if (Get-Command cloudflared -ErrorAction SilentlyContinue) {
    $cloudflaredInstalled = $true
    Write-Host "[3/7] Cloudflared already installed globally, skipping..." -ForegroundColor Gray
}

if (-not $cloudflaredInstalled) {
    Write-Host "[3/7] Installing Cloudflared tunnel client..." -ForegroundColor Yellow

    # Attempt 1: Try Winget
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        winget install --id Cloudflare.cloudflared --exact --accept-package-agreements --accept-source-agreements | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Get-Command cloudflared -ErrorAction SilentlyContinue)) {
            $cloudflaredInstalled = $true
            $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
            Write-Host "  Cloudflared installed via winget!" -ForegroundColor Green
        }
    }

    # Attempt 2: Direct download
    if (-not $cloudflaredInstalled) {
        Write-Host "  Downloading directly from Cloudflare..." -ForegroundColor DarkGray
        $termoteBinDir = "$installDir\bin"
        $cloudflaredPath = "$termoteBinDir\cloudflared.exe"
        $backendCloudflaredPath = "$backendDir\cloudflared.exe"

        if (-not (Test-Path $termoteBinDir)) { New-Item -Type Directory -Force $termoteBinDir | Out-Null }

        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe" -OutFile $cloudflaredPath

        if (Test-Path $cloudflaredPath) {
            Copy-Item $cloudflaredPath -Destination $backendCloudflaredPath -Force
            $env:Path += ";$termoteBinDir"
            $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
            if ($userPath -notmatch [regex]::Escape($termoteBinDir)) {
                [Environment]::SetEnvironmentVariable("Path", "$userPath;$termoteBinDir", "User")
            }
            $cloudflaredInstalled = $true
            Write-Host "  Cloudflared downloaded manually!" -ForegroundColor Green
        } else {
            Write-Host "ERROR: Failed to download cloudflared." -ForegroundColor Red
        }
    }
}

# 4. Compile the Rust backend
Write-Host "[4/7] Compiling Rust backend..." -ForegroundColor Yellow
Set-Location $backendDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust compilation failed." -ForegroundColor Red
    exit 1
}
Write-Host "  Backend compiled successfully!" -ForegroundColor Green

# 5. Create shim directory and files
Write-Host "[5/7] Setting up termote commands..." -ForegroundColor Yellow

if (-not (Test-Path $shimDir)) {
    New-Item -Type Directory -Force $shimDir | Out-Null
}

$termoteShimContent = @"
# Smart termote launcher - VS Code style single instance
$backendDir = "$env:USERPROFILE\termote\backend"
$envFile = "$backendDir\.env"
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
        Write-Host "Termote is ready. Opening new tab at: $cwd" -ForegroundColor Cyan
        $sent = Send-IpcCommand "open_dir:$cwd"

        if ($sent) {
            # Open the browser to the existing session
            if (Test-Path $envFile) {
                $content = Get-Content $envFile -Raw
                $tunnelUrl = if ($content -match 'TUNNEL_URL=(.+)') { $Matches[1].Trim() } else { $null }
                $token = if ($content -match 'AUTH_TOKEN=(.+)') { $Matches[1].Trim() } else { $null }
                if ($tunnelUrl -and $token -and $tunnelUrl -notmatch '127\.0\.0\.1') {
                    # First open raw tunnel URL to clear Cloudflare challenge, then open app
                    $httpsUrl = "https://" + $tunnelUrl.Substring(5)
                    Start-Process $httpsUrl
                    Start-Sleep -Seconds 2
                    $launchUrl = "https://termote.vercel.app/?tunnel=$([Uri]::EscapeDataString($tunnelUrl))&token=$([Uri]::EscapeDataString($token))"
                    Start-Process $launchUrl
                }
            }
            exit 0
        } else {
            Write-Host "Failed to talk to existing Termote instance. It might be frozen." -ForegroundColor Red
        }
    } else {
        Write-Host "Existing Termote instance is hung and won't respond. Restarting it..." -ForegroundColor Yellow
        Stop-Process -Name "termote" -Force -EA SilentlyContinue
        Stop-Process -Name "cloudflared" -Force -EA SilentlyContinue
        Start-Sleep -Seconds 1
    }
}

# 2. If we get here, no instances are running. Start fresh!
Write-Host "Starting fresh Termote server..." -ForegroundColor Green
& "$termoteDir\start.ps1"
"@
Set-Content -Path "$shimDir\termote.ps1" -Value $termoteShimContent -Encoding UTF8

$cmdLines = @(
    "@echo off"
    "powershell -NoProfile -ExecutionPolicy Bypass -Command `"`& '%~dp0termote.ps1' %*`""
)
Set-Content -Path "$shimDir\termote.cmd" -Value $cmdLines -Encoding ASCII

# 6. Add "Open with Termote" context menu
Write-Host "[6/7] Adding Windows Explorer context menu..." -ForegroundColor Yellow

$termoteFileHandler = "$shimDir\termote-file.ps1"
$handlerLines = @(
    '$dir = Split-Path -Parent $args[0]'
    'Start-Process powershell -ArgumentList "-NoExit","-Command","Set-Location $dir; termote" -WindowStyle Normal'
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
Write-Host "[7/7] Starting Termote server..." -ForegroundColor Yellow

Write-Host ""
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Installation complete!" -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Cyan
Write-Host ""
Write-Host "  Available commands:" -ForegroundColor White
Write-Host "  - termote         : Start or connect to Termote" -ForegroundColor Cyan
Write-Host "  - termote-kill    : Stop all Termote instances" -ForegroundColor Cyan
Write-Host "  - Right-click in folder -> 'Open with Termote'" -ForegroundColor Cyan
Write-Host ""
Write-Host "  If commands not found in new terminal, run:" -ForegroundColor Yellow
Write-Host '    $env:PATH = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")' -ForegroundColor Gray
Write-Host ""

& "$installDir\start.ps1"