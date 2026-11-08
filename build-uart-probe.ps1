param([Parameter(Mandatory=$true)][string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
$savedLocation = Get-Location
$savedPath = $env:PATH
$savedLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$steps = [System.Collections.Generic.List[object]]::new()
$transcribing = $false
function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    $code = $LASTEXITCODE
    $steps.Add([ordered]@{executable=$Executable;arguments=$Arguments;exit_code=$code})
    if ($code -ne 0) { throw "$Executable exited with code $code" }
}
try {
    Set-Location -LiteralPath $PSScriptRoot
    $captureTools = if (Test-Path -LiteralPath 'tools/boot-log/capture_boot_log.py') { 'tools/boot-log' } else { '../live-hardware' }
    $destination = [System.IO.Path]::GetFullPath($OutputDirectory,$PSScriptRoot)
    if (Test-Path -LiteralPath $destination) { throw 'Output must be new' }
    New-Item -ItemType Directory -Path $destination | Out-Null
    Start-Transcript -LiteralPath (Join-Path $destination 'build-log.txt') | Out-Null
    $transcribing = $true
    $env:PATH = (Join-Path $env:USERPROFILE '.cargo/bin')+';'+$env:PATH
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'rustc unavailable' }
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $sysroot 'lib/rustlib/x86_64-pc-windows-gnullvm/bin/rust-lld.exe'
    Invoke-Checked 'cargo' @('fmt','-p','uv98-uart-probe','--','--check')
    Invoke-Checked 'rtk' @('cargo','build','--locked','--offline','--release','--target','thumbv7em-none-eabi','-p','uv98-uart-probe')
    $elf = 'target/thumbv7em-none-eabi/release/uv98-uart-probe'
    $audit = Join-Path $destination 'linked-audit.json'
    Invoke-Checked 'python' @('tools/check_uart_probe.py',$elf,'--report',$audit)
    Invoke-Checked 'python' @('tools/export_uart_probe.py',$elf,$audit,(Join-Path $destination 'image'))
    Invoke-Checked 'python' @('tools/verify_uart_probe_capture.py',$audit,(Join-Path $destination 'capture-integration.json'))
    Invoke-Checked 'python' @('-m','unittest','discover','-s',$captureTools,'-p','test_capture_boot_log.py','-q')
    $preflight = & python research/recovery/image_preflight.py (Join-Path $destination 'image/UV98-uart-probe-0.1.2.bin')
    if ($LASTEXITCODE -ne 0) { throw 'Independent BIN preflight failed' }
    $preflight | Set-Content -LiteralPath (Join-Path $destination 'image/bin-preflight.json') -Encoding utf8
    [ordered]@{status='passed';package='uv98-uart-probe';version='0.1.2';commands=$steps;
        device_accessed=$false;flash_performed=$false;physical_hardware_verified=$false;
        built_at_utc=(Get-Date).ToUniversalTime().ToString('o')} |
        ConvertTo-Json -Depth 9 | Set-Content -LiteralPath (Join-Path $destination 'verification.json') -Encoding utf8
} finally {
    if ($transcribing) { Stop-Transcript | Out-Null }
    $env:PATH=$savedPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=$savedLinker
    Set-Location -LiteralPath $savedLocation
}
