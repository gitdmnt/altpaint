# altpaint 起動時間計測スクリプト
# 使い方: .\scripts\measure-startup.ps1
#
# アプリが起動して UI が完全に表示されたら Enter を押してください。
# 経過秒数が出力されるので、ブラックボックステストシートの「起動時間」欄に入力します。

param(
    [string]$ExePath = ""
)

# ── 実行ファイルの自動検索 ──────────────────────────────
if (-not $ExePath) {
    $candidates = @(
        ".\target\release\altpaint.exe",
        ".\target\debug\altpaint.exe",
        ".\target\release\desktop.exe",
        ".\target\debug\desktop.exe"
    )
    foreach ($c in $candidates) {
        if (Test-Path $c) { $ExePath = $c; break }
    }
}

if (-not $ExePath -or -not (Test-Path $ExePath)) {
    Write-Host "実行ファイルが見つかりません。パスを引数で指定してください:" -ForegroundColor Yellow
    Write-Host "  .\scripts\measure-startup.ps1 -ExePath .\target\release\altpaint.exe" -ForegroundColor Cyan
    exit 1
}

Write-Host ""
Write-Host "altpaint 起動時間計測" -ForegroundColor Cyan
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
Write-Host "実行ファイル: $ExePath" -ForegroundColor Gray
Write-Host ""
Write-Host "計測を開始します..."  -ForegroundColor White
Write-Host ""

$sw = [System.Diagnostics.Stopwatch]::StartNew()
Start-Process -FilePath $ExePath

Write-Host "アプリが起動しました。"                             -ForegroundColor Green
Write-Host "UI が完全に表示されたら Enter を押してください..."  -ForegroundColor Yellow
Write-Host ""
Read-Host | Out-Null

$sw.Stop()
$sec = [math]::Round($sw.Elapsed.TotalSeconds, 2)

Write-Host ""
Write-Host "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor DarkGray
Write-Host "起動時間: $sec 秒" -ForegroundColor Cyan
Write-Host ""
Write-Host "テストシートの「起動時間」欄に $sec を入力してください。" -ForegroundColor White
Write-Host ""

# クリップボードにコピー（オプション）
try {
    Set-Clipboard -Value $sec
    Write-Host "($sec をクリップボードにコピーしました)" -ForegroundColor DarkGray
} catch {
    # クリップボードが使えない環境では無視
}
