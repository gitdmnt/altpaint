# plugins

このディレクトリは、`altpaint` のパネルプラグイン開発用ルートです。

各プラグインは独立フォルダを持ち、次を同居させます。

- `Cargo.toml`
- `src/lib.rs`
- `panel.altp-panel`
- 生成された `.wasm`

詳細な開発手順は [../docs/PLUGIN_DEVELOPMENT.md](../docs/PLUGIN_DEVELOPMENT.md) を参照してください。
