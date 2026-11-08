param()
$ErrorActionPreference = 'Stop'
$projectRoot = $PSScriptRoot
$previousLocation = Get-Location
$previousPath = $env:PATH
$previousLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER
$summaryPath = Join-Path $projectRoot 'validation\cps-build-summary.json'
$started = (Get-Date).ToUniversalTime().ToString('o')

function Invoke-Checked {
    param([string]$Executable, [string[]]$Arguments)
    & $Executable @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Executable exited with code $LASTEXITCODE" }
}

try {
    Set-Location -LiteralPath $projectRoot
    New-Item -ItemType Directory -Path 'validation' -Force | Out-Null
    @{ status='running'; started_at=$started; device_accessed=$false } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        $env:PATH = (Join-Path $env:USERPROFILE '.cargo\bin') + ';' + $env:PATH
    }
    $info = & rustc -vV
    if ($LASTEXITCODE -ne 0) { throw 'Rust is unavailable' }
    $hostLine = $info | Where-Object { $_ -match '^host: ' }
    $hostTriple = $hostLine.Substring(6)
    if ($hostTriple -notlike '*windows*') { throw 'This packaging script targets Windows' }
    $sysroot = & rustc --print sysroot
    if ($LASTEXITCODE -ne 0) { throw 'Cannot find Rust sysroot' }
    if ($hostTriple -eq 'x86_64-pc-windows-gnullvm') {
        $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $sysroot 'lib\rustlib\x86_64-pc-windows-gnullvm\bin\rust-lld.exe'
        Invoke-Checked 'python' @('tools/prepare_windows_link.py', '--sysroot', $sysroot)
    }
    Invoke-Checked 'cargo' @('fmt', '--all', '--', '--check')
    $runner = if (Get-Command rtk -ErrorAction SilentlyContinue) { 'rtk' } else { 'cargo' }
    [string[]]$prefix = @()
    if ($runner -eq 'rtk') { $prefix = @('cargo') }
    Invoke-Checked $runner ($prefix + @('test','--locked','-p','uv98-calibration-edit','-p','uv98-codeplug','-p','uv98-cps-core','-p','uv98-radio-read','-p','uv98-cps','--target',$hostTriple))
    Invoke-Checked $runner ($prefix + @('build','--locked','-p','uv98-cps','--release','--target',$hostTriple))
    $exe = Join-Path $projectRoot "target\$hostTriple\release\uv98-cps.exe"
    $portable = Join-Path $projectRoot 'target\cps-portable'
    New-Item -ItemType Directory -Path $portable -Force | Out-Null
    Copy-Item -LiteralPath $exe -Destination (Join-Path $portable 'UV98-CPS.exe')
    $importsJson = & python tools/pe_imports.py $exe
    if ($LASTEXITCODE -ne 0) { throw 'Cannot inspect application imports' }
    $importsJson | Set-Content -LiteralPath 'validation/cps-imports.json' -Encoding utf8
    $imports = $importsJson | ConvertFrom-Json
    if ($imports.imports -contains 'libunwind.dll') {
        $runtime = Join-Path $sysroot "lib\rustlib\$hostTriple\bin\libunwind.dll"
        Copy-Item -LiteralPath $runtime -Destination (Join-Path $portable 'libunwind.dll')
    }
    [ordered]@{
        status='passed'; started_at=$started; finished_at=(Get-Date).ToUniversalTime().ToString('o')
        host=$hostTriple; rustc=$info; device_accessed=$false
        app_sha256=(Get-FileHash -LiteralPath $exe -Algorithm SHA256).Hash.ToLowerInvariant()
        app_bytes=(Get-Item -LiteralPath $exe).Length
        ui_validation='separate manual/Computer Use check required'
    } | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $summaryPath -Encoding utf8
    Write-Host "Portable CPS built: $portable"
}
catch {
    @{ status='failed'; started_at=$started; error=$_.Exception.Message; device_accessed=$false } | ConvertTo-Json | Set-Content -LiteralPath $summaryPath -Encoding utf8
    throw
}
finally {
    $env:PATH = $previousPath
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = $previousLinker
    Set-Location -LiteralPath $previousLocation.Path
}
