#!/usr/bin/env bash
set -euo pipefail

# This script mirrors the behavior of auto-pull-review-merge-my-ideas.ps1 in bash.
# It pulls the latest main, checks for diffs under a specific folder, and
# runs claude to review changes when diffs are detected.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

prev_hash="$(git rev-parse HEAD)"
git pull origin main

# 差分チェック
if ! git diff --quiet "$prev_hash" HEAD -- "特定のフォルダパス"; then
  echo "差分を検知しました。Claude Codeを実行します。"
  claude "docs/memo/以下の変更を確認し、レビューしてください。十分な質が確認されたら、適切なドキュメントにマージしてください。マージ後、ドキュメントのレビューを行い、必要なら実装計画を作成してください。"
else
  echo "差分はありません。"
fi
