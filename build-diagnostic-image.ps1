param(
    [string]$OutputDirectory = '',
    [string]$NorBackup = '../../outputs/uv98-backup-2026-11-08/storage-read-01/storage-address-space.bin',
    [switch]$UpperExoTxTrial,
    [switch]$ChannelTx,
    [switch]$Pa5Audio,
    [ValidateSet('ru', 'en', 'de')]
    [string]$Language = 'ru'
)

# Offline build and file simulation only. No serial/flashing tool is invoked.
$ErrorActionPreference = 'Stop'
$originalLocation = Get-Location
$savedPath = $env:PATH
$savedLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$transcribing = $false
$steps = [System.Collections.Generic.List[object]]::new()
$started = (Get-Date).ToUniversalTime().ToString('o')

function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    $code = $LASTEXITCODE
    $steps.Add([ordered]@{ executable = $Executable; arguments = $Arguments; exit_code = $code })
    if ($code -ne 0) { throw "$Executable exited with code $code" }
}

try {
    if ($UpperExoTxTrial -and $ChannelTx) { throw 'Select exactly one TX profile.' }
    if ($Pa5Audio -and ($UpperExoTxTrial -or $ChannelTx)) { throw 'PA5 audio requires an RX-only build.' }
    Set-Location -LiteralPath $PSScriptRoot
    $captureTools = if (Test-Path -LiteralPath 'tools/boot-log/capture_boot_log.py') { 'tools/boot-log' } else { '../live-hardware' }
    if (-not $OutputDirectory) {
        $OutputDirectory = Join-Path 'validation' ('diag-image-build-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
    }
    $destination = [System.IO.Path]::GetFullPath($OutputDirectory, $PSScriptRoot)
    if (Test-Path -LiteralPath $destination) { throw 'Output directory must be new.' }
    $nor = (Resolve-Path -LiteralPath $NorBackup).Path
    $fixture = 'crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin'
    $expectedNor = '1e8f4c5c5373392a08a041da9132e22d21ad0dba4fca97b9bb4f21ade77c55a3'
    $expectedCal = '8906c3d9847eeeefe0ee6aad6aa4302949bd5e5984f3c64ed02edb8cf6fb0ff7'
    if ((Get-FileHash -LiteralPath $nor).Hash.ToLowerInvariant() -ne $expectedNor) {
        throw 'Expected the preserved 4 MiB user NOR backup; its SHA256 does not match.'
    }
    if ((Get-FileHash -LiteralPath $fixture).Hash.ToLowerInvariant() -ne $expectedCal) {
        throw 'Preserved calibration fixture SHA256 mismatch.'
    }
    New-Item -ItemType Directory -Path $destination | Out-Null
    Start-Transcript -LiteralPath (Join-Path $destination 'build-log.txt') | Out-Null
    $transcribing = $true
    $env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
    $rustInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'rustc unavailable' }
    $hostTriple = ($rustInfo | Where-Object { $_ -match '^host: ' }).Substring(6)
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'rustc sysroot unavailable' }
    if ($hostTriple -eq 'x86_64-pc-windows-gnullvm') {
        $rustHostBin = Join-Path $sysroot "lib/rustlib/$hostTriple/bin"
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $rustHostBin 'rust-lld.exe'
        $env:PATH = $rustHostBin + ';' + $env:PATH
    }
    $runner = if (Get-Command rtk -ErrorAction SilentlyContinue) { 'rtk' } else { 'cargo' }
    [string[]]$prefix = if ($runner -eq 'rtk') { @('cargo') } else { @() }
    $packages = @('uv98-settings-store', 'uv98-smart-beacon', 'uv98-signaling', 'uv98-gps-at32', 'uv98-spectrum', 'uv98-gps-nmea', 'uv98-radio-runtime', 'uv98-rf-sequence', 'uv98-sidekeys-at32', 'uv98-handheld-ui-state', 'uv98-handheld-ui', 'uv98-handheld-app', 'uv98-handheld-image-sim', 'uv98-boot-log', 'uv98-diag-startup', 'uv98-diag-quiesce', 'uv98-diag-app',
        'uv98-board-pins', 'uv98-rf-at32', 'uv98-diagnostic', 'uv98-display',
        'uv98-keys', 'uv98-keyscan-at32', 'uv98-rf-bus', 'uv98-nor-read',
        'uv98-board-at32', 'uv98-lcd-at32', 'uv98-clock-profile', 'uv98-board-diagnostic',
        'uv98-board-policy', 'uv98-diag-image-sim')
    [string[]]$packageArgs = @()
    foreach ($package in $packages) { $packageArgs += @('-p', $package) }
    Invoke-Checked 'cargo' (@('fmt') + $packageArgs + @('-p', 'uv98-diag-firmware', '--', '--check'))
    Invoke-Checked 'python' @('tools/check_oracles.py')
    Invoke-Checked 'python' @('tools/check_board_oracles.py')
    Invoke-Checked $runner ($prefix + @('test', '--locked', '--offline', '--target', $hostTriple) + $packageArgs)
    foreach ($test in @('test_check_elf.py', 'test_check_settings_journal.py', 'test_check_flash_budget.py', 'test_build_release.py', 'test_export_diagnostic_image.py', 'test_package_diagnostic_image.py')) {
        Invoke-Checked 'python' @('-m', 'unittest', 'discover', '-s', 'tools', '-p', $test, '-v')
    }
    Invoke-Checked 'python' @('-m', 'unittest', 'discover', '-s', 'research/recovery', '-p', 'test_image_preflight.py', '-q')
    Invoke-Checked 'python' @('-m', 'unittest', 'discover', '-s', $captureTools, '-p', 'test_capture_boot_log.py', '-v')
    if ($Pa5Audio) {
        Invoke-Checked $runner ($prefix + @('test', '--locked', '--offline', '--target', $hostTriple,
            '-p', 'uv98-handheld-ui', '-p', 'uv98-handheld-app', '-p', 'uv98-boot-log',
            '--features', 'uv98-handheld-app/pa5-audio,uv98-boot-log/pa5-audio'))
        Invoke-Checked 'python' @('tools/pa5-audio/test_decode_pa5.py')
    }
    [string[]]$firmwareFeatures = @()
    [string[]]$featureArgs = @()
    if ($ChannelTx) { $firmwareFeatures = @('channel-tx') }
    if ($UpperExoTxTrial) { $firmwareFeatures = @('upper-exo-tx-trial') }
    if ($Pa5Audio) { $firmwareFeatures = @('pa5-audio') }
    $firmwareLanguage = $Language.ToLowerInvariant()
    $firmwareFeatures += "locale-$firmwareLanguage"
    $featureArgs = @('--features', ($firmwareFeatures -join ','))
    Invoke-Checked $runner ($prefix + @('build', '--locked', '--offline', '--release', '--target', 'thumbv7em-none-eabi', '-p', 'uv98-diag-firmware') + $featureArgs)
    $linkedElf = 'target/thumbv7em-none-eabi/release/uv98-diag-firmware'
    $budgetProfile = if ($ChannelTx) { 'channel-tx' }
        elseif ($UpperExoTxTrial) { 'upper-exo-trial' }
        elseif ($Pa5Audio) { 'pa5-audio' }
        else { 'rx' }
    $imageDirectory = Join-Path $destination 'image'
    $stagedElf = Join-Path $destination 'uv98-diag-firmware.staged.elf'
    try {
        Copy-Item -LiteralPath $linkedElf -Destination $stagedElf
        Invoke-Checked 'python' @('tools/check_flash_budget.py', $stagedElf,
            '--language', $firmwareLanguage, '--profile', $budgetProfile,
            '--report', (Join-Path $destination 'flash-budget.json'))
        Invoke-Checked 'python' @('tools/export_diagnostic_image.py', $stagedElf, $imageDirectory)
    } finally {
        if (Test-Path -LiteralPath $stagedElf) { Remove-Item -LiteralPath $stagedElf }
    }
    Invoke-Checked 'python' @('tools/pack_ui_locales.py', '--check', '--image', (Join-Path $imageDirectory 'UV98-diag-0.1.8.bin'), '--language', $firmwareLanguage)
    Invoke-Checked 'python' @('tools/check_diag_preinit.py', (Join-Path $imageDirectory 'UV98-diag-0.1.8.elf'), '--report', (Join-Path $imageDirectory 'early-machine-code-check.json'))
    [string[]]$journalArgs = @('tools/check_settings_journal.py',
        (Join-Path $imageDirectory 'UV98-diag-0.1.8.elf'), (Join-Path $imageDirectory 'UV98-diag-0.1.8.bin'))
    if ($Pa5Audio) { $journalArgs += '--diagnostic' }
    $journalReport = & python @journalArgs
    if ($LASTEXITCODE -ne 0) { throw 'Settings journal ELF/BIN validation failed' }
    $journalReport | Set-Content -LiteralPath (Join-Path $imageDirectory 'settings-journal-check.json') -Encoding utf8
    if (-not $Pa5Audio) {
        Invoke-Checked 'python' @('crates/settings-store/tests/flash_arm.py', (Join-Path $imageDirectory 'UV98-diag-0.1.8.elf'))
    }
    Invoke-Checked 'python' @('tools/check_device_profile.py', (Join-Path $imageDirectory 'UV98-diag-0.1.8.elf'), '--report', (Join-Path $imageDirectory 'device-profile-check.json'))
    Invoke-Checked 'python' @((Join-Path $captureTools 'verify_boot_log_emulation.py'), '--report', (Join-Path $imageDirectory 'early-machine-code-check.json'), '--output', (Join-Path $destination 'boot-log-emulation-decode'))
    $preflight = & python research/recovery/image_preflight.py (Join-Path $imageDirectory 'UV98-diag-0.1.8.bin')
    if ($LASTEXITCODE -ne 0) { throw 'BIN preflight failed' }
    $preflight | Set-Content -LiteralPath (Join-Path $imageDirectory 'bin-preflight.json') -Encoding utf8
    $map = Get-ChildItem -Path 'target/thumbv7em-none-eabi/release/build/uv98-diag-firmware-*/out/diagnostic.map' |
        Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1
    if (-not $map) { throw 'Linker map missing' }
    Copy-Item -LiteralPath $map.FullName -Destination (Join-Path $imageDirectory 'diagnostic.map')
    Invoke-Checked 'cargo' @('run', '--locked', '--offline', '--release', '-p', 'uv98-handheld-image-sim', '--', $nor, (Join-Path $destination 'preview'))
    if ((Get-FileHash -LiteralPath $nor).Hash.ToLowerInvariant() -ne $expectedNor -or
        (Get-FileHash -LiteralPath $fixture).Hash.ToLowerInvariant() -ne $expectedCal) {
        throw 'Preserved input changed during build.'
    }
    [ordered]@{
        schema = 1; status = 'passed'; started_at = $started
        completed_at = (Get-Date).ToUniversalTime().ToString('o')
        rustc = $rustInfo; target = 'thumbv7em-none-eabi'
        package = 'uv98-diag-firmware'; commands = $steps
        firmware_features = $firmwareFeatures; firmware_language = $firmwareLanguage
        preserved_nor_sha256 = $expectedNor; preserved_calibration_sha256 = $expectedCal
        preserved_inputs_unchanged = $true; device_accessed = $false; flash_performed = $false
        hardware_qualified = $false; recovery_verified = $false
        scope = 'Host tests, actual linked ARM image, ELF/BIN structure, saved-NOR app simulation'
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $destination 'verification.json') -Encoding utf8
    Write-Output "Diagnostic image and verification: $destination"
} finally {
    if ($transcribing) { Stop-Transcript | Out-Null }
    $env:PATH = $savedPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $savedLinker
    Set-Location -LiteralPath $originalLocation
}
