#!/usr/bin/env pwsh
# profile-canvas.ps1 — canvas ops（stamp / stroke / fill / erase）の回帰計測スクリプト
#
# 使い方:
#   .\scripts\profile-canvas.ps1 [-Release] [-Iterations <n>] [-OutputDir <dir>]
#
# 出力:
#   logs/profile-canvas-<timestamp>.json

param(
    [switch]$Release,
    [int]$Iterations = 100,
    [string]$OutputDir = "logs"
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path $PSScriptRoot -Parent

$BuildArgs = @("build", "-p", "canvas")
if ($Release) { $BuildArgs += "--release" }
Write-Host "[profile-canvas] Building..."
& cargo @BuildArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Write-Host "[profile-canvas] Running canvas tests ($Iterations iterations)..."
$TestArgs = @("test", "-p", "canvas", "--", "--nocapture")
if ($Release) { $TestArgs = @("test", "-p", "canvas", "--release", "--", "--nocapture") }

$StartTime = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 1; $i -le $Iterations; $i++) {
    & cargo @TestArgs 2>&1 | Out-Null
}
$StartTime.Stop()

$TotalMs = $StartTime.ElapsedMilliseconds
$AvgMs   = [math]::Round($TotalMs / $Iterations, 2)

$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$OutFile = Join-Path $RootDir "$OutputDir/profile-canvas-$Timestamp.json"
New-Item -ItemType Directory -Force -Path (Split-Path $OutFile) | Out-Null

$Result = [PSCustomObject]@{
    profile    = "canvas"
    timestamp  = (Get-Date -Format "o")
    iterations = $Iterations
    total_ms   = $TotalMs
    avg_ms     = $AvgMs
    release    = $Release.IsPresent
}
$Result | ConvertTo-Json | Set-Content $OutFile

Write-Host "[profile-canvas] Done. avg=$AvgMs ms/iter  total=${TotalMs}ms"
Write-Host "[profile-canvas] Output: $OutFile"
