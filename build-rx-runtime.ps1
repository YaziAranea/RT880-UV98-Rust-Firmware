param()
$ErrorActionPreference = 'Stop'
$previousLocation = Get-Location
$started = (Get-Date).ToUniversalTime().ToString('o')
try {
    Set-Location -LiteralPath $PSScriptRoot
    & ./build-receive.ps1
    if ($LASTEXITCODE -ne 0) { throw "Receive build returned $LASTEXITCODE" }
    $core = Get-Content validation/build-summary.json -Raw | ConvertFrom-Json
    $receive = Get-Content validation/receive-build-summary.json -Raw | ConvertFrom-Json
    if ($core.status -ne 'offline_checks_passed' -or $receive.status -ne 'offline_checks_passed') { throw 'Missing successful prerequisite build' }
    $hashes = [ordered]@{}
    $paths = @('crates/receive','crates/rf-at32','crates/rf-bus','crates/board-pins','crates/band-plan','crates/calibration','crates/stock-model')
    foreach ($path in $paths) {
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File | Sort-Object FullName) {
            $relative = [IO.Path]::GetRelativePath($PSScriptRoot,$file.FullName).Replace('\','/')
            $hashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
    foreach ($file in @('Cargo.toml','Cargo.lock','build.ps1','build-receive.ps1','build-rx-runtime.ps1')) {
        $hashes[$file] = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    [ordered]@{
        status='offline_checks_passed'; started_at=$started; verified_at=(Get-Date).ToUniversalTime().ToString('o')
        source_sha256=$hashes; host=$receive.host; target='thumbv7em-none-eabi'
        arm_hardware_features='uv98-board-diagnostic/hardware,uv98-keyscan-at32/hardware,uv98-rf-at32/hardware'
        calibration_sha256=$receive.calibration_sha256; stage0_elf_sha256=$core.elf_sha256
        oracle_integrity='board-oracle-integrity.json'; hardware_evidence='research/rf-hardware/EVIDENCE-RU.txt'
        runtime_evidence='research/rx-runtime/EVIDENCE-RU.txt'
        device_accessed=$false; flash_performed=$false; ready_to_flash=$false; physical_rf_qualified=$false
        scope='Prepared receiver runtime + opt-in GPIO/PAC backend; host tests and ARM compilation only'
        limitations=@('No cold RF initialization or exact UV98 identity proof','External qualified interlock/clock/bus timing/settling evidence required','Raw RSSI is not calibrated dBm','Stop mutes audio and ends requests; it does not prove RF-off')
    } | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath validation/rx-runtime-build-summary.json -Encoding utf8
    $global:LASTEXITCODE = 0
    Write-Host 'RF GPIO/runtime checks passed. Hardware has not been accessed.'
}
catch {
    [ordered]@{status='failed';started_at=$started;error=$_.Exception.Message;device_accessed=$false;flash_performed=$false} |
        ConvertTo-Json | Set-Content -LiteralPath validation/rx-runtime-build-summary.json -Encoding utf8
    throw
}
finally { Set-Location -LiteralPath $previousLocation.Path }
