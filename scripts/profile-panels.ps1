#!/usr/bin/env pwsh
# profile-panels.ps1 — panel runtime（dsl_panel / registry / host_sync）の回帰計測スクリプト
#
# 使い方:
#   .\scripts\profile-panels.ps1 [-Release] [-Iterations <n>] [-OutputDir <dir>]
#
# 出力:
#   logs/profile-panels-<timestamp>.json

param(
    [switch]$Release,
    [int]$Iterations = 100,
    [string]$OutputDir = "logs"
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path $PSScriptRoot -Parent

$BuildArgs = @("build", "-p", "panel-runtime")
if ($Release) { $BuildArgs += "--release" }
Write-Host "[profile-panels] Building..."
& cargo @BuildArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[profile-panels] Running panel-runtime tests ($Iterations iterations)..."
$TestArgs = @("test", "-p", "panel-runtime", "--", "--nocapture")
if ($Release) { $TestArgs = @("test", "-p", "panel-runtime", "--release", "--", "--nocapture") }

$PerIter = @()
for ($i = 1; $i -le $Iterations; $i++) {
    $Sw = [System.Diagnostics.Stopwatch]::StartNew()
    & cargo @TestArgs 2>&1 | Out-Null
    $Sw.Stop()
    $PerIter += $Sw.ElapsedMilliseconds
}

$TotalMs = ($PerIter | Measure-Object -Sum).Sum
$AvgMs   = [math]::Round($TotalMs / $Iterations, 2)
$MinMs   = ($PerIter | Measure-Object -Minimum).Minimum
$MaxMs   = ($PerIter | Measure-Object -Maximum).Maximum

$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$OutFile = Join-Path $RootDir "$OutputDir/profile-panels-$Timestamp.json"
New-Item -ItemType Directory -Force -Path (Split-Path $OutFile) | Out-Null

$Result = [PSCustomObject]@{
    profile    = "panels"
    timestamp  = (Get-Date -Format "o")
    iterations = $Iterations
    total_ms   = $TotalMs
    avg_ms     = $AvgMs
    min_ms     = $MinMs
    max_ms     = $MaxMs
    release    = $Release.IsPresent
}
$Result | ConvertTo-Json | Set-Content $OutFile

Write-Host "[profile-panels] Done. avg=$AvgMs ms  min=$MinMs ms  max=$MaxMs ms"
Write-Host "[profile-panels] Output: $OutFile"
