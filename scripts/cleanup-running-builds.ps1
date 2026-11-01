[CmdletBinding()]
param([switch]$Preview)

$ErrorActionPreference = 'Stop'
$workspacePath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$distPath = Join-Path $workspacePath 'dist'
if (-not (Test-Path -LiteralPath $distPath -PathType Container)) { return }
$directory = Get-Item -LiteralPath $distPath
if ($directory.Attributes -band [System.IO.FileAttributes]::ReparsePoint) {
    throw 'Refusing to clean a redirected distribution directory.'
}
$resolvedDist = [System.IO.Path]::GetFullPath($directory.FullName)
$activePaths = @{}
foreach ($process in Get-Process -ErrorAction SilentlyContinue) {
    try {
        $processPath = $process.Path
        if ($processPath) { $activePaths[[System.IO.Path]::GetFullPath($processPath)] = $true }
    } catch { }
}
$removed = 0
$kept = 0
$bytes = [long]0
foreach ($file in Get-ChildItem -LiteralPath $resolvedDist -File) {
    if ($file.Name -notmatch '^Axibiz(?: Desktop Beta)?-running-\d{14}(?:\d{3})?\.exe$') { continue }
    $candidate = [System.IO.Path]::GetFullPath($file.FullName)
    if ([System.IO.Path]::GetDirectoryName($candidate) -ne $resolvedDist) { throw 'Cleanup path escaped the distribution directory.' }
    if (($file.Attributes -band [System.IO.FileAttributes]::ReparsePoint) -or $activePaths.ContainsKey($candidate)) {
        $kept++
        continue
    }
    try {
        if (-not $Preview) { Remove-Item -LiteralPath $candidate -ErrorAction Stop }
        $removed++
        $bytes += $file.Length
    } catch {
        # Windows also protects mapped executables when a renamed process reports its old path.
        $kept++
    }
}
[pscustomobject]@{ Preview = [bool]$Preview; RemovedOrEligible = $removed; Kept = $kept; Megabytes = [math]::Round($bytes / 1MB, 2) }
