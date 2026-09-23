# README-LEGACY: 旧コード (legacy-final) のビルドとテスト

altpaint は ADR 020 ([docs/adr/020-codebase-rebuild-strategy.md](docs/adr/020-codebase-rebuild-strategy.md)) の方針で、骨格を新しく作り直す。
作り直す前の旧コード (ブランチ refactor/ui-theme-and-clipping の af4ec88) は `legacy-final` タグで凍結し、この文書にビルドとテストの手順を残す。
旧コードの既知の不具合は ADR 020 の 2 章にある。再現テストは `legacy-repro-gpu` / `legacy-repro-nongpu` タグにある。

## 必要なもの

- Rust 1.94.0 と `wasm32-unknown-unknown` ターゲット。`rust-toolchain.toml` で固定してあり、rustup が自動で入れる。
- Python 3。stylo (Blitz の CSS エンジン) の build.rs が使う。`python3` が PATH で見つからない環境 (Windows など) では、環境変数 `PYTHON3` に python の実行ファイルのパスを入れる。
  - 例 (PowerShell): `$env:PYTHON3 = "C:\Users\<user>\AppData\Local\Programs\Python\Python311\python.exe"`
- wgpu が使える GPU。GPU が無い環境では、GPU を使うテストの一部が何もせずにパスする (ADR 020 2 章の 24)。
- 追跡している `Cargo.lock` をそのまま使う。blitz 系は `0.3.0-alpha.2` に解決されている必要があり、`cargo update` をしたり Cargo.lock なしで解決したりするとビルドできない。

## 手順

1. 同梱パネルの Wasm をビルドする。`.wasm` は追跡していないが、パネルを読み込むテストはビルド済みの `.wasm` を使う。
   - Windows (PowerShell): `.\scripts\build-ui-wasm.ps1`
   - Linux / WSL2: `bash scripts/build-ui-wasm.sh`。このスクリプトは workspace-layout パネルを含まないので、続けて次を実行する。
     ```bash
     cargo build --target wasm32-unknown-unknown -p builtin-panel-workspace-layout
     cp target/wasm32-unknown-unknown/debug/builtin_panel_workspace_layout.wasm crates/builtin-panels/workspace-layout/
     ```
2. ビルドする: `cargo build`
3. テストする: `cargo test --workspace`
4. 起動する: `cargo run -p altpaint-desktop`

## テストの記録

| 時点 | 環境 | passed / failed / ignored |
|---|---|---|
| B10 の記録 (docs/IMPLEMENTATION_STATUS.md:174、2026-06-14) | 記録なし | 568 / 0 / 9 |
