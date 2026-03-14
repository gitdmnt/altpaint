#!/usr/bin/env pwsh
# profile-render.ps1 — render パスの回帰計測スクリプト
#
# 使い方:
#   .\scripts\profile-render.ps1 [-Release] [-Iterations <n>] [-OutputDir <dir>]
#
# 出力:
#   logs/profile-render-<timestamp>.json  — 計測結果 JSON

param(
    [switch]$Release,
    [int]$Iterations = 100,
    [string]$OutputDir = "logs"
)

$ErrorActionPreference = "Stop"
$RootDir = Split-Path $PSScriptRoot -Parent

# ビルド
$BuildArgs = @("build", "-p", "desktop")
if ($Release) { $BuildArgs += "--release" }
Write-Host "[profile-render] Building..."
& cargo @BuildArgs
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

# テスト実行（render 関連テストのみ）
Write-Host "[profile-render] Running render tests ($Iterations iterations)..."
$TestArgs = @("test", "-p", "render", "--", "--nocapture")
if ($Release) { $TestArgs = @("test", "-p", "render", "--release", "--", "--nocapture") }

$StartTime = [System.Diagnostics.Stopwatch]::StartNew()
for ($i = 1; $i -le $Iterations; $i++) {
    & cargo @TestArgs 2>&1 | Out-Null
}
$StartTime.Stop()

$TotalMs = $StartTime.ElapsedMilliseconds
$AvgMs   = [math]::Round($TotalMs / $Iterations, 2)

# 出力
$Timestamp = Get-Date -Format "yyyyMMdd-HHmmss"
$OutFile = Join-Path $RootDir "$OutputDir/profile-render-$Timestamp.json"
New-Item -ItemType Directory -Force -Path (Split-Path $OutFile) | Out-Null

$Result = [PSCustomObject]@{
    profile   = "render"
    timestamp = (Get-Date -Format "o")
    iterations = $Iterations
    total_ms  = $TotalMs
    avg_ms    = $AvgMs
    release   = $Release.IsPresent
}
$Result | ConvertTo-Json | Set-Content $OutFile

Write-Host "[profile-render] Done. avg=$AvgMs ms/iter  total=${TotalMs}ms"
Write-Host "[profile-render] Output: $OutFile"
