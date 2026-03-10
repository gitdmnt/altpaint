# Pen Presets

## この文書の目的

この文書は、`altpaint` におけるペンプリセットの最小仕様、配置場所、将来の importer 方針をまとめる。

現段階では **Photoshop / CSP のネイティブ形式をそのまま実行時正本にしない**。
`altpaint` 側で扱う正本は、`pens/` 配下の `*.altp-pen.json` である。

## 現在の実装

2026-03-10 時点では次を実装している。

- `pens/` 配下を再帰探索して `*.altp-pen.json` を読み込む
- `pens/` 配下を再帰探索して `*.altp-pen.json` / `*.abr` / `*.sut` / `*.gbr` を読み込む
- 起動時に既定ペンディレクトリを読み込む
- `builtin.tool-palette` から `Reload Pens` を押して再読込できる
- `builtin.tool-palette` から前/次のプリセットへ切り替えられる
- `builtin.pen-settings` から現在のペン幅を変更できる
- `Pen` ツールは可変幅ストロークを描ける
- `storage` に `AltPaintPen` 正規化 IR と parse/export module がある
- Photoshop `ABR` sampled brush の最小 importer がある
- Clip Studio Paint `SUT` の SQLite metadata importer がある
- `GIMP GBR` の parse/export module がある
- `pens/abr/` や `pens/sut/` に置いた外部ブラシも起動時/再読込時に取り込める

## 既定ディレクトリ

- 既定ディレクトリ: [pens](../pens)
- 既定拡張子: `.altp-pen.json`

例:

- [pens/round-pen.altp-pen.json](../pens/round-pen.altp-pen.json)
- [pens/fine-liner.altp-pen.json](../pens/fine-liner.altp-pen.json)
- [pens/broad-marker.altp-pen.json](../pens/broad-marker.altp-pen.json)

## ファイル形式

### ver.1 互換形式

```json
{
  "format_version": 1,
  "id": "builtin.round-pen",
  "name": "Round Pen",
  "size": 4,
}
```

### ver.2 正規化形式

```json
{
  "format_version": 2,
  "id": "imported.abr.ink-1",
  "name": "Imported Ink",
  "engine": "stamp",
  "base_size": 24.0,
  "min_size": 1.0,
  "max_size": 128.0,
  "spacing_percent": 25.0,
  "opacity": 1.0,
  "flow": 1.0,
  "pressure_enabled": true,
  "antialias": true,
  "stabilization": 0,
  "tip": {
    "kind": "alpha-mask8",
    "width": 128,
    "height": 128,
    "data_base64": "..."
  },
  "dynamics": {
    "size_pressure_curve": {
      "points": [
        { "x": 0.0, "y": 0.0 },
        { "x": 1.0, "y": 1.0 }
      ]
    }
  },
  "source": {
    "kind": "photoshop-abr",
    "original_file": "ink.abr",
    "notes": []
  }
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

### ver.2

- `format_version`
  - 現在は `2`
- `id`
  - 一意なペン ID
- `name`
  - UI 表示名
- `engine`
  - 現在は `stamp` / `generated`
- `base_size`
  - 基本サイズ
- `min_size` / `max_size`
  - importer が推定した許容レンジ
- `spacing_percent`
  - スタンプ間隔
- `opacity` / `flow`
  - 0.0〜1.0 の正規化値
- `pressure_enabled`
  - 筆圧を使うか
- `tip`
  - `alpha-mask8` / `rgba8` / `png-blob`
- `dynamics`
  - 現在は pressure curve などの正規化先
- `source`
  - 元形式と degrade 情報
- `extras`
  - 未正規化の補助情報

## Photoshop / CSP 調査まとめ

### Photoshop

- 主なブラシ形式名は `ABR`
- 公式に安定した公開仕様として扱いやすい形式ではない
- 実務上は reverse engineering 依存が強い
- 完全互換より `ABR -> altpaint pen preset` importer の方が現実的
- 現在は sampled brush を中心に `ABR v1/v2/v6` を最小 import 対象にする
- computed brush は degrade policy として skip する

### Clip Studio Paint

- 主なサブツール/ブラシ形式名は `SUT`
- 公式な公開仕様としては扱いづらい
- 素材参照や内部 DB 依存が強く、完全互換のコストが高い
- 完全互換より `SUT -> altpaint pen preset` importer の方が現実的
- 非公式調査ベースでは `.sut` を SQLite DB とみなし、`Node` / `Variant` / `MaterialFile` を読む方針が現実的
- 現在は metadata / pressure graph / material PNG metadata の抽出までを最小対応とする

## 現在の parse/export module

- `storage::parse_altpaint_pen_json(...)`
  - `ver.1` / `ver.2` の `*.altp-pen.json` を読む
- `storage::parse_photoshop_abr_bytes(...)`
  - Photoshop `ABR` を正規化 `AltPaintPen` 群へ落とす
- `storage::parse_clip_studio_sut(...)`
  - Clip Studio Paint `SUT` を read-only で調査し、正規化 metadata を返す
- `storage::parse_gimp_gbr_bytes(...)`
  - `GIMP GBR` を読む
- `storage::export_altpaint_pen_json(...)`
  - 正規化 `AltPaintPen` を `*.altp-pen.json` として書き出す
- `storage::export_gimp_gbr(...)`
  - 埋め込み brush tip を `GBR` へ書き出す

## 推奨方針

1. 実行時正本は `altpaint` 独自の `*.altp-pen.json` に固定する
2. Photoshop / CSP は importer を別段で用意する
3. importer の責務は「完全再現」ではなく「近似変換」に置く
4. 将来は先端画像、散布、筆圧カーブなどを段階的に拡張する

## 次段階

- `SUT` 内の material binding 精度向上
- `ABR` descriptor section から spacing 等をより正確に復元
- 散布、texture、tilt/velocity 系 dynamics の正規化追加
- ペン一覧 UI の拡張
