Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $repoRoot

$prevHash = git rev-parse HEAD
git pull origin main

# 差分チェック
git diff --quiet $prevHash HEAD -- "特定のフォルダパス"
if ($LASTEXITCODE -ne 0) {
    Write-Host "差分を検知しました。Claude Codeを実行します。"
    claude "`docs/memo/`以下の変更を確認し、レビューしてください。十分な質が確認されたら、適切なドキュメントにマージしてください。マージ後、ドキュメントのレビューを行い、必要なら実装計画を作成してください。"
}
else {
    Write-Host "差分はありません。"
}