# Pen presets

`altpaint` は `pens/` 配下の `*.altp-pen.json` を起動時と再読込時に読み込みます。

最小フォーマット:

```json
{
  "format_version": 1,
  "id": "round-pen",
  "name": "Round Pen",
  "size": 4,
  "min_size": 1,
  "max_size": 64
}
```

現段階では Photoshop / CSP のネイティブ形式を直接保存しません。
将来の importer は、この内部フォーマットへ変換して配置する前提です。
