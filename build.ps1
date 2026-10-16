param([switch]$Offline, [switch]$Check, [switch]$PackageOnly, [string]$Python = 'python')
$ErrorActionPreference = 'Stop'
$pluginArguments = @((Join-Path $PSScriptRoot 'build.py'))
if ($Offline) { $pluginArguments += '--offline' }
if ($Check) { $pluginArguments += '--check' }
if ($PackageOnly) { $pluginArguments += '--package-only' }
& $Python @pluginArguments
if ($LASTEXITCODE -ne 0) { throw 'Depth Studio build failed.' }
