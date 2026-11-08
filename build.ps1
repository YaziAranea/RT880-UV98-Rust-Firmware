param()

$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$savedPath = $env:PATH
$savedHostLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$originalLocation = Get-Location
$summaryPath = Join-Path $projectRoot 'validation\build-summary.json'
$startedAt = (Get-Date).ToUniversalTime().ToString('o')

function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Executable exited with code $LASTEXITCODE"
    }
}

try {
    Set-Location -LiteralPath $projectRoot
    New-Item -ItemType Directory -Path 'validation' -Force | Out-Null
    [ordered]@{
        status = 'running'
        started_at = $startedAt
        hardware_qualified = $false
        flash_performed = $false
    } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        if (-not (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe'))) {
            throw 'Rust is missing. Install Rust and the thumbv7em-none-eabi target first.'
        }
        $env:PATH = $cargoBin + ';' + $env:PATH
    }

    $rustInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'rustc -vV failed' }
    $hostLine = $rustInfo | Where-Object { $_ -match '^host: ' }
    if (-not $hostLine) { throw 'Could not detect Rust host' }
    $hostTriple = $hostLine.Substring(6)
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Could not detect Rust sysroot' }

    # Pure Rust programs can use the bundled linker on this Windows host.
    # This is process-local; no global Rust, PATH, or linker configuration changes.
    if ($hostTriple -eq 'x86_64-pc-windows-gnullvm') {
        $bundledLinker = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-gnullvm\bin\rust-lld.exe'
        if (-not (Test-Path -LiteralPath $bundledLinker)) {
            throw 'The Rust gnullvm bundled linker is missing.'
        }
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $bundledLinker
        Invoke-Checked 'python' @('tools/prepare_windows_link.py', '--sysroot', $sysroot)
    }

    $targetLib = & rustc --print target-libdir --target thumbv7em-none-eabi
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $targetLib)) {
        throw 'Run rustup target add thumbv7em-none-eabi before building.'
    }
    New-Item -ItemType Directory -Path 'validation' -Force | Out-Null
    $fixture = 'crates/calibration/tests/fixtures/uv98-plus-stock-1.16.bin'
    $expectedFixtureHash = '8906c3d9847eeeefe0ee6aad6aa4302949bd5e5984f3c64ed02edb8cf6fb0ff7'
    if ((Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expectedFixtureHash) {
        throw 'Calibration fixture SHA256 does not match the preserved backup.'
    }
    Invoke-Checked 'python' @('tools/check_oracles.py')
    Invoke-Checked 'cargo' @('fmt', '--all', '--', '--check')
    $cargoRunner = if (Get-Command rtk -ErrorAction SilentlyContinue) { 'rtk' } else { 'cargo' }
    [string[]]$cargoPrefix = @()
    if ($cargoRunner -eq 'rtk') { $cargoPrefix = @('cargo') }
    Invoke-Checked 'python' @('tools/check_board_oracles.py')
    Invoke-Checked $cargoRunner ($cargoPrefix + @('test', '--locked', '-p', 'uv98-calibration', '-p', 'uv98-calibration-edit', '-p', 'uv98-band-plan', '-p', 'uv98-rf-bus', '-p', 'uv98-receive', '-p', 'uv98-rf-at32', '-p', 'uv98-stock-model', '-p', 'uv98-codeplug', '-p', 'uv98-cps-core', '-p', 'uv98-nor-read', '-p', 'uv98-board-at32', '-p', 'uv98-display', '-p', 'uv98-keys', '-p', 'uv98-diagnostic', '-p', 'uv98-diag-sim', '-p', 'uv98-board-pins', '-p', 'uv98-clock-profile', '-p', 'uv98-lcd-at32', '-p', 'uv98-board-policy', '-p', 'uv98-board-diagnostic', '-p', 'uv98-keyscan-at32', '-p', 'uv98-spectrum', '-p', 'uv98-ui-core', '-p', 'uv98-radio-ui', '-p', 'uv98-radio-sim', '--target', $hostTriple))
    Invoke-Checked $cargoRunner ($cargoPrefix + @('build', '--locked', '-p', 'uv98-stage0', '-p', 'uv98-band-plan', '-p', 'uv98-rf-bus', '-p', 'uv98-receive', '-p', 'uv98-rf-at32', '-p', 'uv98-stock-model', '-p', 'uv98-codeplug', '-p', 'uv98-nor-read', '-p', 'uv98-board-at32', '-p', 'uv98-display', '-p', 'uv98-keys', '-p', 'uv98-diagnostic', '-p', 'uv98-board-policy', '-p', 'uv98-board-diagnostic', '-p', 'uv98-keyscan-at32', '-p', 'uv98-ui-core', '-p', 'uv98-spectrum', '-p', 'uv98-radio-ui', '--features', 'uv98-board-diagnostic/hardware,uv98-keyscan-at32/hardware,uv98-rf-at32/hardware', '--release', '--target', 'thumbv7em-none-eabi'))
    $elfPath = 'target/thumbv7em-none-eabi/release/uv98-stage0'
    Invoke-Checked 'python' @('tools/check_elf.py', $elfPath, '--output', 'validation/elf-layout.json')
    Invoke-Checked 'python' @('-m', 'unittest', 'discover', '-s', 'tools', '-p', 'test_check_elf.py', '-v')
    Invoke-Checked 'python' @('-m', 'unittest', 'discover', '-s', 'research/recovery', '-p', 'test_image_preflight.py', '-q')

    # This program reads a local file only. Its output contains stored codes.
    $report = & cargo run --locked --quiet -p uv98-calibration --example inspect --target $hostTriple -- $fixture
    if ($LASTEXITCODE -ne 0) { throw 'Calibration inspection failed' }
    $report | Set-Content -LiteralPath 'validation/calibration-report.txt' -Encoding utf8
    $modelReport = & cargo run --locked --quiet -p uv98-stock-model --example inspect_model --target $hostTriple -- $fixture
    if ($LASTEXITCODE -ne 0) { throw 'Offline calibration model inspection failed' }
    $modelReport | Set-Content -LiteralPath 'validation/stock-model-report.txt' -Encoding utf8
    $summary = [ordered]@{
        status = 'offline_checks_passed'
        started_at = $startedAt
        hardware_qualified = $false
        flash_performed = $false
        verified_at = (Get-Date).ToUniversalTime().ToString('o')
        rustc = $rustInfo
        host = $hostTriple
        target = 'thumbv7em-none-eabi'
        elf_sha256 = (Get-FileHash -LiteralPath $elfPath -Algorithm SHA256).Hash.ToLowerInvariant()
        fixture_sha256 = (Get-FileHash -LiteralPath $fixture -Algorithm SHA256).Hash.ToLowerInvariant()
        stock_model_reference_sha256 = 'eaef215cc7084dcd94281bbffb7ab6e2dd1e79ce135f01c90e3dbc691dd16bfc'
        exact_uv98_application_identity_verified = $false
    }
    $summary | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath 'validation/build-summary.json' -Encoding utf8
    Write-Host 'Offline checks passed. This stage is not qualified for flashing.'
}
catch {
    [ordered]@{
        status = 'failed'
        started_at = $startedAt
        finished_at = (Get-Date).ToUniversalTime().ToString('o')
        hardware_qualified = $false
        flash_performed = $false
        error = $_.Exception.Message
    } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
    throw
}
finally {
    $env:PATH = $savedPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $savedHostLinker
    Set-Location -LiteralPath $originalLocation.Path
}
