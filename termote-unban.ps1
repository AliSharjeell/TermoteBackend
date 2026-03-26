# Termote Unban Script
# Unbans a previously banned IP address

param(
    [Parameter(Mandatory=$true)]
    [string]$IpAddress
)

function Send-IpcCommand($cmd) {
    try {
        $client = New-Object System.Net.Sockets.TcpClient
        $client.Connect("127.0.0.1", 9091)
        $client.ReceiveTimeout = 3000
        $stream = $client.GetStream()
        $reader = New-Object System.IO.StreamReader($stream)
        $writer = New-Object System.IO.StreamWriter($stream)
        $writer.WriteLine($cmd)
        $writer.Flush()
        $writer.Dispose()
        $response = $reader.ReadToEnd()
        $stream.Dispose()
        $client.Close()
        return $response.Trim()
    } catch {
        return $null
    }
}

Write-Host ""
Write-Host "  Termote Unban" -ForegroundColor Cyan
Write-Host "  ────────────" -ForegroundColor Cyan
Write-Host ""

if (-not $IpAddress) {
    Write-Host "Usage: termote-unban <ip-address>" -ForegroundColor Yellow
    Write-Host "Example: termote-unban 192.168.1.100" -ForegroundColor Gray
    exit 1
}

Write-Host "Unbanning IP: $IpAddress" -ForegroundColor White

$response = Send-IpcCommand "unban:$IpAddress"
if ($null -eq $response) {
    Write-Host "Failed to connect to Termote backend." -ForegroundColor Red
    Write-Host "Is Termote running?" -ForegroundColor Yellow
    exit 1
}

if ($response -eq "IP unbanned successfully") {
    Write-Host "  Successfully unbanned $IpAddress" -ForegroundColor Green
} elseif ($response -eq "IP was not banned") {
    Write-Host "  IP $IpAddress was not in the ban list" -ForegroundColor Yellow
} else {
    Write-Host "  Response: $response" -ForegroundColor Gray
}
Write-Host ""
