param([switch]$Offline, [string]$Python = 'python')
$ErrorActionPreference = 'Stop'
$arguments = @((Join-Path $PSScriptRoot 'fetch-assets.py'))
if ($Offline) { $arguments += '--offline' }
& $Python @arguments
if ($LASTEXITCODE -ne 0) { throw 'Depth Studio runtime verification/restoration failed.' }
