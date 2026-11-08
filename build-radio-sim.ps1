param()

# Build the Windows host simulator only. No application launch or device I/O.
$previousErrorAction = $ErrorActionPreference
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$previousLocation = Get-Location
$previousPath = $env:PATH
$previousLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$previousTargetDirectory = $env:CARGO_TARGET_DIR
$summaryPath = Join-Path $projectRoot 'validation\radio-sim-build-summary.json'
$started = (Get-Date).ToUniversalTime().ToString('o')

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
        started_at = $started
        device_accessed = $false
        app_launched = $false
    } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -or
        -not (Get-Command rustc -ErrorAction SilentlyContinue)) {
        $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
        if (-not (Test-Path -LiteralPath (Join-Path $cargoBin 'cargo.exe')) -or
            -not (Test-Path -LiteralPath (Join-Path $cargoBin 'rustc.exe'))) {
            throw 'Rust is unavailable. Install a Windows Rust host toolchain first.'
        }
        $env:PATH = $cargoBin + ';' + $env:PATH
    }
    if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
        throw 'Python is required for local linker preparation and PE import inspection.'
    }
    $rustInfo = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'rustc -vV failed' }
    $hostLine = @($rustInfo | Where-Object { $_ -match '^host: ' })
    if ($hostLine.Count -ne 1) { throw 'Could not determine the Rust host target' }
    $hostTriple = $hostLine[0].Substring(6).Trim()
    # pe_imports.py currently validates PE32+ only.
    if ($hostTriple -notmatch '^(x86_64|aarch64)-pc-windows-(gnullvm|gnu|msvc)$') {
        throw "Unsupported host target: $hostTriple. This helper builds a 64-bit Windows simulator."
    }
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $sysroot)) {
        throw 'Could not find the active Rust sysroot'
    }

    # Keep every build/runtime artifact inside this project's target directory,
    # even if the calling shell has a different CARGO_TARGET_DIR configured.
    $env:CARGO_TARGET_DIR = Join-Path $projectRoot 'target'
    if ($hostTriple -eq 'x86_64-pc-windows-gnullvm') {
        $linker = Join-Path $sysroot "lib\rustlib\$hostTriple\bin\rust-lld.exe"
        if (-not (Test-Path -LiteralPath $linker)) { throw 'Bundled Rust linker is missing' }
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $linker
        Invoke-Checked 'python' @('tools/prepare_windows_link.py', '--sysroot', $sysroot)
    }

    $runner = if (Get-Command rtk -ErrorAction SilentlyContinue) { 'rtk' } else { 'cargo' }
    [string[]]$prefix = @()
    if ($runner -eq 'rtk') { $prefix = @('cargo') }
    Invoke-Checked $runner ($prefix + @('build', '--locked', '--release', '-p', 'uv98-radio-sim', '--target', $hostTriple))

    $relativeExe = "target/$hostTriple/release/uv98-radio-sim.exe"
    $exe = Join-Path $projectRoot $relativeExe
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        throw 'Cargo succeeded but the expected simulator executable is absent'
    }
    $importsJson = & python tools/pe_imports.py $exe
    if ($LASTEXITCODE -ne 0) { throw 'Simulator PE import inspection failed' }
    $imports = $importsJson | ConvertFrom-Json
    $importsJson | Set-Content -LiteralPath 'validation/radio-sim-imports.json' -Encoding utf8

    $runtimeHash = $null
    $runtimePath = $null
    if ($imports.imports -contains 'libunwind.dll') {
        $runtimeSource = Join-Path $sysroot "lib\rustlib\$hostTriple\bin\libunwind.dll"
        if (-not (Test-Path -LiteralPath $runtimeSource -PathType Leaf)) {
            throw 'Simulator imports libunwind.dll, but the active host sysroot does not provide it'
        }
        $runtimeDestination = Join-Path (Split-Path -Parent $exe) 'libunwind.dll'
        Copy-Item -LiteralPath $runtimeSource -Destination $runtimeDestination -Force
        $runtimeHash = (Get-FileHash -LiteralPath $runtimeDestination -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($runtimeHash -ne (Get-FileHash -LiteralPath $runtimeSource -Algorithm SHA256).Hash.ToLowerInvariant()) {
            throw 'Copied simulator runtime SHA256 mismatch'
        }
        $runtimePath = "target/$hostTriple/release/libunwind.dll"
    }

    [ordered]@{
        status = 'passed'
        started_at = $started
        finished_at = (Get-Date).ToUniversalTime().ToString('o')
        host = $hostTriple
        rustc = $rustInfo
        build_scope = 'host simulator only; GUI verification and tests are separate'
        device_accessed = $false
        app_launched = $false
        hardware_qualified = $false
        executable = $relativeExe
        app_sha256 = (Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        app_bytes = (Get-Item -LiteralPath $exe).Length
        runtime = $runtimePath
        runtime_sha256 = $runtimeHash
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding utf8
    Write-Host "Host simulator built: $exe"
}
catch {
    $buildError = $_
    try {
        [ordered]@{
            status = 'failed'
            started_at = $started
            finished_at = (Get-Date).ToUniversalTime().ToString('o')
            device_accessed = $false
            app_launched = $false
            error = $buildError.Exception.Message
        } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
    }
    catch { Write-Warning 'Could not write the failed simulator build report.' }
    throw $buildError
}
finally {
    $env:PATH = $previousPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $previousLinker
    $env:CARGO_TARGET_DIR = $previousTargetDirectory
    Set-Location -LiteralPath $previousLocation.Path
    $ErrorActionPreference = $previousErrorAction
}
