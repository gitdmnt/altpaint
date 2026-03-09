# Pen Presets

## この文書の目的

この文書は、`altpaint` におけるペンプリセットの最小仕様、配置場所、将来の importer 方針をまとめる。

現段階では **Photoshop / CSP のネイティブ形式をそのまま実行時正本にしない**。
`altpaint` 側で扱う正本は、`pens/` 配下の `*.altp-pen.json` である。

## 現在の実装

2026-03-09 時点では次を実装している。

- `pens/` 配下を再帰探索して `*.altp-pen.json` を読み込む
- 起動時に既定ペンディレクトリを読み込む
- `builtin.tool-palette` から `Reload Pens` を押して再読込できる
- `builtin.tool-palette` から前/次のプリセットへ切り替えられる
- `builtin.pen-settings` から現在のペン幅を変更できる
- `Pen` ツールは可変幅ストロークを描ける

## 既定ディレクトリ

- 既定ディレクトリ: [pens](../pens)
- 既定拡張子: `.altp-pen.json`

例:

- [pens/round-pen.altp-pen.json](../pens/round-pen.altp-pen.json)
- [pens/fine-liner.altp-pen.json](../pens/fine-liner.altp-pen.json)
- [pens/broad-marker.altp-pen.json](../pens/broad-marker.altp-pen.json)

## 最小ファイル形式

```json
{
  "format_version": 1,
  "id": "builtin.round-pen",
  "name": "Round Pen",
  "size": 4,
}
```

## フィールド

### ver.1

- `format_version`
  - 現在は `1` 固定
- `id`
  - 一意なプリセット ID
- `name`
  - UI 表示名
- `size`
  - 初期幅

## Photoshop / CSP 調査まとめ

### Photoshop

- 主なブラシ形式名は `ABR`
- 公式に安定した公開仕様として扱いやすい形式ではない
- 実務上は reverse engineering 依存が強い
- 完全互換より `ABR -> altpaint pen preset` importer の方が現実的

### Clip Studio Paint

- 主なサブツール/ブラシ形式名は `SUT`
- 公式な公開仕様としては扱いづらい
- 素材参照や内部 DB 依存が強く、完全互換のコストが高い
- 完全互換より `SUT -> altpaint pen preset` importer の方が現実的

## 推奨方針

1. 実行時正本は `altpaint` 独自の `*.altp-pen.json` に固定する
2. Photoshop / CSP は importer を別段で用意する
3. importer の責務は「完全再現」ではなく「近似変換」に置く
4. 将来は先端画像、散布、筆圧カーブなどを段階的に拡張する

## 次段階

- `ABR` の sampled brush 限定 importer
- `SUT` の最小抽出 importer
- 筆圧カーブ、散布、テクスチャの IR 拡張
- ペン一覧 UI の拡張
