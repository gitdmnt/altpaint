# Rotation renderer follow-up (2026-03-10)

このセッションでは、ユーザー依頼に従って**プラグイン周辺のみ**を変更し、renderer には触れていない。

## 未対応として残した内容

### 1. 回転時に画像が歪む

- 症状: 非 90 度系の回転でキャンバス内容が歪んで見える
- 原因候補:
  - `render` 側の回転表現が依然として quarter turn ベースになっている
  - `CanvasScene` / texture quad / software blit のいずれかが任意角回転を前提にしていない

### 2. 回転角を真に無段階化したい

- 現状:
  - plugin / command / host snapshot 側には `rotation_degrees` がある
  - ただし実描画側が任意角回転を正しく扱えていない
- 必要作業:
  - `render::CanvasScene` の回転モデルを quarter turn 依存から外す
  - dirty rect / UV / hit test / brush preview / lasso overlay の変換を任意角対応に揃える
  - WGPU 表示経路と software 合成経路の両方で同じ回転モデルを使う

## 触る想定の主な箇所

- `crates/render/src/lib.rs`
- `apps/desktop/src/frame/geometry.rs`
- `apps/desktop/src/frame/raster.rs`
- `apps/desktop/src/wgpu_canvas.rs`

## このセッションで完了した plugin 側対応

- pen size の数値直接入力追加
- tool palette 側での per-tool/per-pen size memory の保持と復元フック追加
- view plugin の slider 即時 state 反映追加

renderer 修正は別セッションで対応すること。

## 追加残置タスク: SDK からの render pass 割り込み / filter layer 拡張

ユーザー要望として、SDK から render pass に割り込み、filter layer や post effect を差し込める拡張ポイントが必要になった。
ただしこれは plugin SDK だけでは完結せず、renderer / host ABI / 実行モデルの見直しが必要なので、このセッションでは未実装とした。

### 必要そうな設計作業

- `panel-sdk` / 将来の plugin SDK で表現する render hook API の設計
  - 例: pre-composite / per-layer / post-composite のどこへ割り込めるか
  - effect parameter の受け渡し形式
  - 読み取り専用 texture と書き込み先 texture の契約
- `plugin-host` 側で Wasm effect を安全に呼び出す ABI 設計
  - render thread 上で直接呼ぶか、command buffer 形式にするか
  - timeout / fault isolation / fallback policy
- `render` 側の pass graph / layer compose 経路の拡張
  - filter layer を通常レイヤーとどう混在させるか
  - dirty rect とキャッシュを effect aware に再設計するか
- `app-core` / `storage` 側のドキュメントモデル拡張
  - filter layer の永続化形式
  - effect instance の parameter schema

### 主に触る想定箇所

- `crates/panel-sdk/`
- `crates/plugin-host/`
- `crates/plugin-api/`
- `crates/render/src/lib.rs`
- `apps/desktop/src/frame/`
- `crates/storage/`
- `crates/app-core/`

### 備考

- 単なる SDK API 追加ではなく、renderer 側の pass 管理と lifetime 契約が本体になる
- 現状の panel plugin runtime は UI/command 中心なので、そのままでは filter layer 実行基盤としては不足している