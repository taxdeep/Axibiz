param([switch]$SkipChecks)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
if (-not (Test-Path -LiteralPath $cargoPath)) { throw 'Rust Cargo was not found.' }
function Invoke-Cargo {
    param([string[]]$CargoArgs)
    & $cargoPath @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "Cargo failed: $CargoArgs" }
}
if (-not $SkipChecks) {
    Invoke-Cargo -CargoArgs @('fmt','--check')
    Invoke-Cargo -CargoArgs @('clippy','--all-targets','--','-D','warnings')
    Invoke-Cargo -CargoArgs @('test')
}
Invoke-Cargo -CargoArgs @('build','--release','--target-dir','target/desktop')
$distPath = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Path $distPath -Force | Out-Null
$destination = Join-Path $distPath 'Axibiz Desktop Beta.exe'
$running = Get-Process -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $destination }
if ($running) { Rename-Item -LiteralPath $destination -NewName ('Axibiz Desktop Beta-running-' + (Get-Date -Format 'yyyyMMddHHmmssfff') + '.exe') }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'target\desktop\release\axibiz.exe') -Destination $destination -Force
$legacyExecutable = Join-Path $distPath 'Axibiz.exe'
if (Test-Path -LiteralPath $legacyExecutable) {
    Rename-Item -LiteralPath $legacyExecutable -NewName ('Axibiz-running-' + (Get-Date -Format 'yyyyMMddHHmmssfff') + '.exe')
}
& (Join-Path $PSScriptRoot 'scripts\cleanup-running-builds.ps1')
Write-Host "Built $destination"
