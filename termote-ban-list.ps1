# Termote Ban List Viewer
# Shows all currently banned IP addresses

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
Write-Host "  Termote Banned IPs" -ForegroundColor Cyan
Write-Host "  ─────────────────" -ForegroundColor Cyan
Write-Host ""

$response = Send-IpcCommand "ban-list"
if ($null -eq $response) {
    Write-Host "Failed to connect to Termote backend." -ForegroundColor Red
    Write-Host "Is Termote running?" -ForegroundColor Yellow
    exit 1
}

if ($response -eq "No banned IPs") {
    Write-Host "  No banned IPs" -ForegroundColor Gray
} else {
    $lines = $response -split "`n"
    foreach ($line in $lines) {
        if ($line.Trim()) {
            Write-Host "  $($line.Trim())" -ForegroundColor White
        }
    }
}
Write-Host ""
