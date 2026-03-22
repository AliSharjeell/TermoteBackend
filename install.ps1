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
Write-Host "  Remote terminal accessible from any browser" -ForegroundColor Gray
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

# 3. Download cloudflared into the backend folder
$backendDir = "$installDir\backend"
$cloudflaredPath = "$backendDir\cloudflared.exe"
if (-not (Test-Path $cloudflaredPath)) {
    Write-Host "[3/6] Downloading Cloudflared tunnel client..." -ForegroundColor Yellow
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-windows-amd64.exe" -OutFile $cloudflaredPath
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path $cloudflaredPath)) {
        Write-Host "ERROR: Failed to download cloudflared. Check your internet connection." -ForegroundColor Red
        exit 1
    }
    Write-Host "  Cloudflared downloaded!" -ForegroundColor Green
} else {
    Write-Host "[3/6] Cloudflared already present, skipping..." -ForegroundColor Gray
}

# 4. Compile the Rust backend
Write-Host "[4/6] Compiling Rust backend (first-time only, ~3-5 min)..." -ForegroundColor Yellow
Write-Host "  This may show no output for a while - that is normal. Rust is compiling." -ForegroundColor DarkGray
Set-Location $backendDir
cargo build --release
if ($LASTEXITCODE -ne 0) {
    Write-Host "ERROR: Rust compilation failed." -ForegroundColor Red
    exit 1
}
Write-Host "  Backend compiled successfully!" -ForegroundColor Green

# 5. Create the global 'termote' PowerShell command
Write-Host "[5/6] Setting up global 'termote' command..." -ForegroundColor Yellow

$profilePath = if ($PROFILE) { $PROFILE } else { "$env:USERPROFILE\Documents\WindowsPowerShell\Microsoft.PowerShell_profile.ps1" }

if (-not (Test-Path (Split-Path $profilePath))) { New-Item -Type Directory -Force (Split-Path $profilePath) | Out-Null }
if (-not (Test-Path $profilePath)) { New-Item -Type File -Force $profilePath | Out-Null }

$aliasLines = @(
    ""
    "function termote {"
    "    `$env:PATH += `";$backendDir`""
    "    Set-Location `"$backendDir`""
    "    .\start.ps1"
    "}"
)

$existingProfile = Get-Content $profilePath -Raw -ErrorAction SilentlyContinue
if ($null -eq $existingProfile -or $existingProfile -notmatch "function termote") {
    $aliasLines | Add-Content -Path $profilePath
}

Write-Host "  Added 'termote' function to your PowerShell profile." -ForegroundColor Green
Write-Host "[6/6] Starting Termote server..." -ForegroundColor Yellow

Write-Host "================================================================" -ForegroundColor Cyan
Write-Host "  Installation complete! Starting server now..." -ForegroundColor Green
Write-Host "================================================================" -ForegroundColor Cyan

# Reload profile and start
. $profilePath
termote
