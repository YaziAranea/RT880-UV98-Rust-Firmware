param(
    [string]$Port = '',
    [string]$Decode = '',
    [string]$Output = '',
    [ValidateRange(5, 90)][int]$Seconds = 30
)
$ErrorActionPreference = 'Stop'
if ([bool]$Port -eq [bool]$Decode) { throw 'Specify exactly one: -Port COMn to listen, or -Decode rx.bin for offline conversion.' }
if (-not $Output) { $Output = Join-Path (Get-Location) ('PA5-' + (Get-Date -Format 'yyyyMMdd-HHmmss')) }
$destination = [IO.Path]::GetFullPath($Output)
if (Test-Path -LiteralPath $destination) { throw 'Output directory must be new.' }
if ($Port -and $Port -notmatch '^COM[1-9][0-9]{0,3}$') { throw 'Select an explicit COM port from Device Manager.' }
Add-Type -Path (Join-Path $PSScriptRoot 'DecodePa5.cs')
New-Item -ItemType Directory -Path $destination | Out-Null
if ($Decode) {
    $source = [IO.File]::OpenRead((Resolve-Path -LiteralPath $Decode).Path)
    try {
        if ($source.Length -gt 1048576) { throw 'Capture exceeds 1 MiB.' }
        $stream = New-Object IO.MemoryStream
        $buffer = New-Object byte[] 1024
        while (($count = $source.Read($buffer, 0, $buffer.Length)) -gt 0) {
            if ($stream.Length + $count -gt 1048576) { throw 'Capture grew beyond 1 MiB.' }
            $stream.Write($buffer, 0, $count)
        }
        $captureBytes = $stream.ToArray(); $stream.Dispose()
    } finally { $source.Dispose() }
} else {
    $serial = New-Object System.IO.Ports.SerialPort
    $serial.PortName = $Port; $serial.BaudRate = 115200
    $serial.Parity = [IO.Ports.Parity]::None; $serial.DataBits = 8
    $serial.StopBits = [IO.Ports.StopBits]::One; $serial.Handshake = [IO.Ports.Handshake]::None
    $serial.DtrEnable = $false; $serial.RtsEnable = $false; $serial.ReadTimeout = 100
    $stream = New-Object IO.MemoryStream
    $buffer = New-Object byte[] 1024
    try {
        $serial.Open()
        Write-Host "LISTENING $Port at 115200. On radio: capture with 5, then press # to export. No serial commands are sent."
        $timer = [Diagnostics.Stopwatch]::StartNew()
        while ($timer.Elapsed.TotalSeconds -lt $Seconds) {
            try { $count = $serial.Read($buffer, 0, $buffer.Length) } catch [TimeoutException] { continue }
            if ($stream.Length + $count -gt 1048576) { throw 'Capture exceeds 1 MiB.' }
            $stream.Write($buffer, 0, $count)
        }
    } finally {
        if ($serial.IsOpen) { $serial.Close() }
        $serial.Dispose()
        $captureBytes = $stream.ToArray(); $stream.Dispose()
        [IO.File]::WriteAllBytes((Join-Path $destination 'rx.bin'), $captureBytes)
    }
}
$reports = [DecodePa5]::Decode($captureBytes, $destination)
$reports | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath (Join-Path $destination 'report.json') -Encoding UTF8
$reports | Format-List
Write-Host "Saved raw ADC, WAV and report: $destination"
