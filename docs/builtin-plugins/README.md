# builtin-plugins

このディレクトリは、altpaint に同梱されているビルトインパネルプラグインの個別メモをまとめる場所です。

現在の対象は以下の6つです。

- [app-actions.md](app-actions.md)
- [tool-palette.md](tool-palette.md)
- [color-palette.md](color-palette.md)
- [layers-panel.md](layers-panel.md)
- [job-progress.md](job-progress.md)
- [snapshot-panel.md](snapshot-panel.md)
- [keyboard-shortcut-config.md](keyboard-shortcut-config.md)

いずれの文書も、2026-03-09 時点の最小実装を前提にしています。

共通事項:

- 実装本体は plugins/<panel-name>/ にあります
- 各パネルフォルダには `.altp-panel` / Rust SDK ソース / 生成 Wasm が同居します
- `.wasm` は生成物として git 管理せず、必要時は scripts/build-ui-wasm.ps1 で再生成します
- ホストとの契約は crates/plugin-api/src/lib.rs にあります
- Rust SDK helper は crates/panel-sdk/src/lib.rs にあります
- 実際の描画、ヒットテスト、フォーカス、スクロールは crates/ui-shell/src/lib.rs 側で行います
- パネル由来の状態変更は HostAction::DispatchCommand(Command) を通じて desktop 側へ渡されます
- キーボード設定の永続化方針は [keyboard-shortcut-config.md](keyboard-shortcut-config.md) を参照してください
- 開発手順の詳細は [../PLUGIN_DEVELOPMENT.md](../PLUGIN_DEVELOPMENT.md) を参照してください
