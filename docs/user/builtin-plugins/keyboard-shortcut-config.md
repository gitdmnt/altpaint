# Keyboard Shortcut Configuration

## 目的

ビルトインパネルや将来の外部パネルが、**自分のキーボード設定を持ち**、かつ**プロジェクトと一緒に永続化**できるようにするためのデータフローを整理する。

ここでは次の2点を分けて扱う。

1. 推奨アーキテクチャ
2. 2026-03-09 時点の最小実装

## 前提

- キャンバス描画はホスト主導である
- パネル UI もホスト主導である
- パネルは `PanelEvent` を受け、`HostAction` または command descriptor を返す
- 状態変更は最終的に `Command` 経由へ寄せたい

## 推奨データフロー

### 1. 入力受信

OS のキーボードイベントは `apps/desktop` が受ける。

その後、ホストはイベントをそのままプラグインへ投げるのではなく、最低でも次へ正規化する。

- `shortcut`
  - 例: `Ctrl+S`, `Ctrl+Shift+S`, `B`, `E`
- `key`
  - 物理または論理的な基準キー名
- `repeat`
  - キーリピートかどうか

これを `PanelEvent::Keyboard` としてパネルランタイムへ流す。

### 2. パネル状態の分割

パネル state は少なくとも 2 つの領域へ分けるのがよい。

- `config.*`
  - 永続化してよい設定
  - 例: `config.save_shortcut = "Ctrl+S"`
- `session.*`
  - 一時状態
  - 例: 現在ショートカットをキャプチャ中か、どの項目を編集中か

これにより、保存対象と非保存対象を UI DSL / Wasm 側で明確に分けられる。

### 3. パネル内の解決

パネルは `keyboard` handler で次の順に処理するのがよい。

1. `session.capture_target` があれば、押された `shortcut` を `config.*` へ保存する
2. そうでなければ、`config.*` と受信 shortcut を照合する
3. 一致したら command descriptor を返す

この構造にすると、ショートカット編集 UI と実際のショートカット発火ロジックを同じプラグイン内へ閉じ込められる。

### 4. ホスト側の永続化

ホストはパネル全状態をそのまま保存するのではなく、**各パネルの `config` 部分だけ**を取り出して保存するのがよい。

推奨形:

```text
project.plugin_configs[panel_id] = { ...config subtree... }
```

理由:

- `session.*` のような一時状態を誤って保存しない
- パネル内部の transient な UI 状態に保存形式が引きずられにくい
- 将来、パネル state を細分化しても保存契約を保ちやすい

### 5. 復元

プロジェクト読込時は、ホストが `plugin_configs[panel_id]` を対象パネルへ戻す。

流れ:

1. `storage` が `plugin_configs` をロードする
2. `apps/desktop` が `UiShell` へ渡す
3. `UiShell` が対象パネルへ `restore_persistent_config(...)` を呼ぶ
4. パネルが `state.config` を復元する

## 将来的に入れたい解決層

最終的には、ホスト側に**ショートカットレジストリ**を持つのが望ましい。

理由:

- 複数パネルが同じ shortcut を宣言したときの競合解決が必要
- フォーカス中パネル優先、ワークスペース優先、グローバル優先などのポリシーが必要
- キャンバス系ショートカットとパネル系ショートカットの衝突を統制したい

推奨順序:

1. フォーカス中パネルの shortcut
2. 明示的に global を宣言したパネル shortcut
3. ホスト既定 shortcut

ただし MVP では、まずパネル自身がショートカットを持てることを優先し、競合 UI は後段でよい。

## このリポジトリでの最小実装

2026-03-09 時点では、次の最小実装を採用している。

- `panel-api::PanelEvent` に `Keyboard` を追加
- `apps/desktop` が `winit` のキー入力を `shortcut` / `key` / `repeat` へ正規化する
- `UiShell` が keyboard 対応パネルへイベントを配送する
- `DslPanelPlugin` は `keyboard` handler export がある Wasm パネルだけへ `PanelEventRequest` を投げる
- 永続化は `storage` の `plugin_configs` へ保存する
- 復元時は `UiShell` が `restore_persistent_config(...)` を呼ぶ

保存されるのはパネルごとの `config` subtree のみであり、`session` subtree は保存しない。

## 既知の制約

- 現在の keyboard 配送は最小実装で、複数パネルの競合解決はまだ持たない
- ホスト既定ショートカットとの優先順位は、将来レジストリ化して整理する余地がある
- command descriptor 側の payload 表現はまだ最小であり、複雑なショートカットメタデータは未対応

## 実装指針まとめ

- 永続化対象は `config.*` に閉じ込める
- 一時 UI 状態は `session.*` に置く
- キーボードイベントはホストで正規化してから送る
- 復元は panel id 単位で行う
- 将来はホスト側ショートカットレジストリを追加する
