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