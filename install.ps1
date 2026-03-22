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

# 1. Clone or update the repo
if (-not (Test-Path $installDir)) {
    Write-Host "[1/6] Cloning Termote repository..." -ForegroundColor Yellow
    git clone --depth 1 $RepoUrl $installDir
    if ($LASTEXITCODE -ne 0) {
        Write-Host "ERROR: Failed to clone repository. Is Git installed?" -ForegroundColor Red
        exit 1
    }
} else {
    Write-Host "[1/6] Updating existing Termote installation..." -ForegroundColor Yellow
    Set-Location $installDir
    git pull origin $Branch
}

# 2. Install Rust if not present
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "[2/6] Installing Rust (first-time only, ~2 min)..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://win.rustup.rs" -OutFile "$env:TEMP\rustup-init.exe"
    & "$env:TEMP\rustup-init.exe" -y -q --default-toolchain stable
    Remove-Item "$env:TEMP\rustup-init.exe" -Force -ErrorAction SilentlyContinue
    $env:Path += ";$env:USERPROFILE\.cargo\bin"
    $env:Path = [System.Environment]::GetEnvironmentVariable("Path","User") + ";" + [System.Environment]::GetEnvironmentVariable("Path","Machine")
    Write-Host "  Rust installed successfully!" -ForegroundColor Green
} else {
    Write-Host "[2/6] Rust already installed, skipping..." -ForegroundColor Gray
}

# 3. Install cloudflared (Winget with direct download fallback)
if (-not (Get-Command cloudflared -ErrorAction SilentlyContinue)) {
    Write-Host "[3/6] Installing Cloudflared tunnel client..." -ForegroundColor Yellow
    
    $installed = $false

    # Attempt 1: Try using Winget first (Cleanest method)
    if (Get-Command winget -ErrorAction SilentlyContinue) {
        Write-Host "  Found winget, attempting installation..." -ForegroundColor DarkGray
        winget install --id Cloudflare.cloudflared --exact --accept-package-agreements --accept-source-agreements | Out-Null
        
        if ($LASTEXITCODE -eq 0 -and (Get-Command cloudflared -ErrorAction SilentlyContinue)) {
            $installed = $true
            # Refresh the PATH variable for the current session
            $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
            Write-Host "  Cloudflared installed successfully via winget!" -ForegroundColor Green
        } else {
            Write-Host "  Winget installation failed. Pivoting to manual download..." -ForegroundColor Yellow
        }
    }

    # Attempt 2: Fallback to direct GitHub download if Winget fails or is missing
    if (-not $installed) {
        Write-Host "  Downloading directly from Cloudflare..." -ForegroundColor DarkGray
        $termoteBinDir = "$installDir\bin"
        $cloudflaredPath = "$termoteBinDir\cloudflared.exe"
        $backendCloudflaredPath = "$backendDir\cloudflared.exe"

        if (-not (Test-Path $termoteBinDir)) { New-Item -Type Directory -Force $termoteBinDir | Out-Null }

        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -Uri "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe" -OutFile $cloudflaredPath

        if (-not (Test-Path $cloudflaredPath)) {
            Write-Host "ERROR: Failed to download cloudflared. Check your internet connection." -ForegroundColor Red
            exit 1
        }

        # Copy to backend folder for start.ps1
        Copy-Item $cloudflaredPath -Destination $backendCloudflaredPath -Force

        # Add to current terminal session PATH
        $env:Path += ";$termoteBinDir"

        # Add to permanent Windows User PATH so it works in all future terminals
        $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if ($userPath -notmatch [regex]::Escape($termoteBinDir)) {
            [Environment]::SetEnvironmentVariable("Path", "$userPath;$termoteBinDir", "User")
        }

        Write-Host "  Cloudflared downloaded manually and added to global PATH!" -ForegroundColor Green
    }
} else {
    Write-Host "[3/6] Cloudflared already installed globally, skipping..." -ForegroundColor Gray
}

# 4. Compile the Rust backend
Write-Host "[4/6] Compiling Rust backend (first-time only, ~3-5 min)..." -ForegroundColor Yellow
Write-Host "  This may show no output for a while - that is normal. Rust is compiling." -ForegroundColor DarkGray
$backendDir = "$installDir\backend"
Set-Location $backendDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust compilation failed." -ForegroundColor Red
    exit 1
}
Write-Host "  Backend compiled successfully!" -ForegroundColor Green

# 5. Create a smart termote.ps1 shim in a permanent location
Write-Host "[5/6] Setting up global termote command..." -ForegroundColor Yellow

$shimDir = "$env:USERPROFILE\.termote-bin"
if (-not (Test-Path $shimDir)) { New-Item -Type Directory -Force $shimDir | Out-Null }

# Write the smart launcher shim (VS Code style - single instance)
$shimPath = "$shimDir\termote.ps1"
$shimContent = @"
# Smart termote launcher - VS Code style single instance
# If termote is running, sends open_dir command via IPC
# If not running, starts termote fresh

`$backendDir = "`$env:USERPROFILE\termote\backend"
`$envFile = "`$backendDir\.env"
`$termoteDir = "`$env:USERPROFILE\termote"

function Send-IpcCommand(`$cmd) {
    try {
        `$client = New-Object System.Net.Sockets.TcpClient
        `$client.Connect("127.0.0.1", 9091)
        `$stream = `$client.GetStream()
        `$writer = New-Object System.IO.StreamWriter(`$stream)
        `$writer.WriteLine(`$cmd)
        `$writer.Flush()
        `$stream.Close()
        `$client.Close()
        return `$true
    } catch {
        return `$false
    }
}

# Get current directory
`$cwd = (Get-Location).Path

# Check if termote is already running
`$isRunning = `$false
try {
    `$resp = Invoke-WebRequest -Uri "http://localhost:9090/health" -TimeoutSec 1 -EA SilentlyContinue
    if (`$resp.StatusCode -eq 200) { `$isRunning = `$true }
} catch { `$isRunning = `$false }

if (`$isRunning) {
    Write-Host "Termote is running. Opening new tab at: `$cwd" -ForegroundColor Cyan
    `$sent = Send-IpcCommand "open_dir:`$cwd"
    if (`$sent) {
        # Open browser to existing session
        if (Test-Path `$envFile) {
            `$content = Get-Content `$envFile -Raw
            `$tunnelUrl = if (`$content -match 'TUNNEL_URL=(.+)') { `$Matches[1].Trim() } else { `$null }
            `$token = if (`$content -match 'AUTH_TOKEN=(.+)') { `$Matches[1].Trim() } else { `$null }
            if (`$tunnelUrl -and `$token) {
                `$launchUrl = "https://termote.vercel.app/?tunnel=`$(`[Uri]::EscapeDataString(`$tunnelUrl))&token=`$(`[Uri]::EscapeDataString(`$token))"
                Start-Process `$launchUrl
            }
        }
        exit 0
    } else {
        Write-Host "IPC failed, starting new instance..." -ForegroundColor Yellow
    }
}

# Start termote
Write-Host "Starting Termote server..." -ForegroundColor Green
& "`$termoteDir\start.ps1"
"@
Set-Content -Path $shimPath -Value $shimContent -Encoding UTF8

# Add shimDir to permanent user PATH if not already there
$userPath = [Environment]::GetEnvironmentVariable("PATH", "User")
if ($userPath -notlike "*$shimDir*") {
    [Environment]::SetEnvironmentVariable("PATH", "$userPath;$shimDir", "User")
    Write-Host "  Added to PATH permanently." -ForegroundColor Green
}

# Load it into current session too
$env:PATH += ";$shimDir"

Write-Host "  Global termote command installed." -ForegroundColor Green

# 6. Add "Open with Termote" to Windows Explorer context menu
Write-Host "[6/7] Adding Windows Explorer context menu..." -ForegroundColor Yellow

$shimPath = "$shimDir\termote.ps1"
$termoteFileHandler = "$shimDir\termote-file.ps1"

# Create a handler script for context menu (just launches termote)
$handlerContent = @"
# Context menu handler - just launches termote
`$dir = Split-Path -Parent `$args[0]
Start-Process powershell -ArgumentList "-NoExit","-Command","Set-Location `$dir; termote" -WindowStyle Normal
"@
Set-Content -Path $termoteFileHandler -Value $handlerContent -Encoding UTF8

# Add registry entries for folders (right-click on folder background)
$regPath = "HKCU:\Software\Classes\Directory\Background\shell\Termote"
$cmdPath = "$regPath\command"

if (-not (Test-Path $regPath)) {
    New-Item -Path $regPath -Force | Out-Null
    Set-ItemProperty -Path $regPath -Name "(Default)" -Value "Open with Termote"
    Set-ItemProperty -Path $regPath -Name "Icon" -Value "`"$shimPath`",0"
}

if (-not (Test-Path $cmdPath)) {
    New-Item -Path $cmdPath -Force | Out-Null
}
Set-ItemProperty -Path $cmdPath -Name "(Default)" -Value "powershell -WindowStyle Hidden -File `"$termoteFileHandler`" `"%V`""

# Add registry entries for directory (right-click on folder icon)
$regPath2 = "HKCU:\Software\Classes\Directory\shell\Termote"
$cmdPath2 = "$regPath2\command"

if (-not (Test-Path $regPath2)) {
    New-Item -Path $regPath2 -Force | Out-Null
    Set-ItemProperty -Path $regPath2 -Name "(Default)" -Value "Open with Termote"
    Set-ItemProperty -Path $regPath2 -Name "Icon" -Value "`"$shimPath`",0"
}

if (-not (Test-Path $cmdPath2)) {
    New-Item -Path $cmdPath2 -Force | Out-Null
}
Set-ItemProperty -Path $cmdPath2 -Name "(Default)" -Value "powershell -WindowStyle Hidden -File `"$termoteFileHandler`" `"%1`""

Write-Host "  Context menu installed (right-click in folder background)." -ForegroundColor Green
Write-Host "[7/7] Starting Termote server..." -ForegroundColor Yellow

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Installation complete! Starting server now..." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Cyan

& "$installDir\start.ps1"