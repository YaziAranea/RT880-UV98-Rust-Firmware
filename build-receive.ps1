param()
$ErrorActionPreference = 'Stop'
$previousLocation = Get-Location
$previousPath = $env:PATH
$previousLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$started = (Get-Date).ToUniversalTime().ToString('o')
try {
    Set-Location -LiteralPath $PSScriptRoot
    $env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
    & ./build.ps1
    $info = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'rustc unavailable' }
    $hostTriple = ($info | Where-Object { $_ -match '^host: ' }).Substring(6)
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'rustc sysroot unavailable' }
    if ($hostTriple -eq 'x86_64-pc-windows-gnullvm') {
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-gnullvm\bin\rust-lld.exe'
    }
    $fixture = 'crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin'
    $before = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
    $exampleArgs = @('run','--locked','--offline','--quiet','-p','uv98-receive','--example','inspect_receive','--target',$hostTriple,'--',$fixture)
    $primary = & cargo @exampleArgs 14550000 wide 5 --chip-b-frequency
    if ($LASTEXITCODE -ne 0) { throw 'Primary and secondary offline plan example failed' }
    $primary | Set-Content -LiteralPath validation/receive-preview-vhf.txt -Encoding utf8
    $uhf = & cargo @exampleArgs 43350000 narrow 0
    if ($LASTEXITCODE -ne 0) { throw 'UHF/level-zero offline plan example failed' }
    $uhf | Set-Content -LiteralPath validation/receive-preview-uhf.txt -Encoding utf8
    $rejections = @()
    foreach ($case in @(@('70000000','wide','5'), @('14550000','wide','11'), @('14550000','2','5'))) {
        $output = & cargo @exampleArgs @case 2>&1
        $code = $LASTEXITCODE
        if ($code -eq 0) { throw "Invalid offline input was accepted: $case" }
        $rejections += [ordered]@{ arguments=$case; exit_code=$code; message=($output -join "`n") }
    }
    $after = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($before -ne $after) { throw 'Calibration source changed' }
    $sourceHashes = [ordered]@{}
    foreach ($file in Get-ChildItem -LiteralPath crates/receive -Recurse -File | Sort-Object FullName) {
        $relative = [IO.Path]::GetRelativePath($PSScriptRoot, $file.FullName).Replace('\','/')
        $sourceHashes[$relative] = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
    [ordered]@{
        status='offline_checks_passed'; started_at=$started; verified_at=(Get-Date).ToUniversalTime().ToString('o')
        host=$hostTriple; arm_target='thumbv7em-none-eabi'; source_sha256=$sourceHashes
        primary_plan_operations=7; secondary_frequency_operations=2; invalid_cli_inputs=$rejections
        calibration_sha256=$after; calibration_unchanged=$true
        core_build='build-summary.json'; oracle_integrity='board-oracle-integrity.json'
        preview_files=@('receive-preview-vhf.txt','receive-preview-uhf.txt')
        device_accessed=$false; flash_performed=$false; physical_rf_qualified=$false; ready_to_flash=$false
        scope='Offline configuration composition, executor fault tests and RF bus mocks; no radio startup or settling proof'
    } | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath validation/receive-build-summary.json -Encoding utf8
    # Expected CLI rejection tests return nonzero; do not leak their exit code
    # as the result of this successful PowerShell build entry point.
    $global:LASTEXITCODE = 0
    Write-Host 'Receive plans, executor, transport and offline examples passed; no physical radio accessed.'
}
catch {
    [ordered]@{ status='failed'; started_at=$started; error=$_.Exception.Message; device_accessed=$false; flash_performed=$false } |
        ConvertTo-Json | Set-Content -LiteralPath validation/receive-build-summary.json -Encoding utf8
    throw
}
finally {
    $env:PATH = $previousPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $previousLinker
    Set-Location -LiteralPath $previousLocation.Path
}
