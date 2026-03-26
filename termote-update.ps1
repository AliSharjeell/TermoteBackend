# Termote Update Script
# Fetches and executes the latest Termote installation script from GitHub

param(
    [switch]$WhatIf
)

$RepoUrl = "https://raw.githubusercontent.com/AliSharjeell/Termote/master/install.ps1"

Write-Host ""
Write-Host "  Termote Update" -ForegroundColor Cyan
Write-Host "  ──────────────" -ForegroundColor Cyan
Write-Host ""

if ($WhatIf) {
    Write-Host "  [WhatIf] Would execute:" -ForegroundColor Yellow
    Write-Host "    irm $RepoUrl | iex" -ForegroundColor Gray
    Write-Host ""
    Write-Host "  This would download and run the latest install.ps1" -ForegroundColor Gray
    Write-Host "  from the Termote GitHub repository." -ForegroundColor Gray
    exit 0
}

Write-Host "  Fetching latest Termote installation script..." -ForegroundColor White
Write-Host ""

try {
    $script = Invoke-RestMethod -Uri $RepoUrl -TimeoutSec 30 -ErrorAction Stop
    if ($script) {
        Write-Host "  Script downloaded successfully." -ForegroundColor Green
        Write-Host "  Executing installation..." -ForegroundColor White
        Write-Host ""
        Invoke-Expression $script
    }
} catch {
    Write-Host "  Failed to fetch update script: $_" -ForegroundColor Red
    Write-Host "  Please check your internet connection and try again." -ForegroundColor Yellow
    exit 1
}
