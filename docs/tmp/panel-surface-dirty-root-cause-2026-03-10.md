# panel surface dirty / 伸び残り不具合の原因調査

## 対象事象

- プラグイン panel を移動すると、端が引き伸ばされたように見える
- 旧位置または新位置の一部が更新されず、残像のように見える
- 発生頻度が高く、特定 panel 固有ではなく host 全体で再現する

## 結論

根本原因は、`ui-shell` が返す `PanelSurface` の座標系と、`apps/desktop` 側 compositor が想定していた座標系が一致していなかったことにある。

- `ui-shell` の `PanelSurface` は **global surface** ではなく、`x` / `y` / `width` / `height` で示される **実際の panel 描画領域つき surface** である
- しかし desktop compositor は、その pixel buffer を panel host 全体へ貼る前提で扱っていた
- 結果として、`PanelSurface` が host viewport 全体サイズではない場合に、部分 surface が host 全域へ拡大転送され、見た目上の伸び・境界残り・不完全更新が発生した

これは dirty rect の局所バグではなく、**surface の意味付けの食い違い** による合成バグである。

## 事実確認

### `ui-shell` 側

`PanelSurface` は panel 群の実表示領域を持つ。

- `x`, `y`: window 内の global offset
- `width`, `height`: 描画済み surface の実サイズ
- `pixels`: その矩形領域の RGBA buffer

この設計では、panel が移動すれば surface の global offset も変わる。
したがって consumer は、pixel buffer を **`PanelSurface` 自身の global bounds に従って** 合成しなければならない。

### `apps/desktop` 側の旧実装

旧 compositor では panel surface を host rect に合わせて貼っていた。

- source: `PanelSurface.pixels`
- destination: panel host rect 全体
- 合成方法: 実質的に「部分 surface を host 全体へ再配置/拡大」する扱い

このため、panel move 後に次の破綻が起きる。

1. 新しい `PanelSurface` は新位置の矩形だけを正しく持つ
2. しかし compositor はそれを host 全体へ貼る
3. panel が存在しない領域にも source edge が引き延ばされる
4. 旧位置の消去と新位置の再描画が視覚的にねじれ、端が残る

## なぜ頻発したか

この不具合は dirty rect の偶発的な取りこぼしではなく、panel を動かすたびに通る合成経路の前提違反だったため、条件が揃えば高頻度で見えた。

特に次の条件で目立つ。

- panel host が広く、`PanelSurface` がその一部しか覆わない
- panel 移動量が小さく、前フレームとの差分が edge に集中する
- background / overlay / panel layer の差分提示で、誤った blit がそのまま残る

## 今回の根本修正

### 1. panel surface 合成を global bounds 基準へ統一

desktop compositor で、`PanelSurface` を host 全体へ scale/blit する経路をやめ、`panel_surface.x`, `panel_surface.y`, `panel_surface.width`, `panel_surface.height` をそのまま destination に使う blit へ変更した。

要点:

- source pixel buffer の意味を変えない
- destination は host rect ではなく `PanelSurface` 自身の global rect
- panel move 時は新旧 dirty union の上に正しい位置だけが再合成される

これにより、surface edge の引き伸ばしは構造的に起こらなくなる。

### 2. canvas 側も page-space presentation へ寄せた

今回の panel 矩形作成ツールと panel 範囲外マスク表示を実現するため、canvas 表示を active panel local bitmap 直貼りから、page-space frame 表示へ変更した。

これにより:

- active panel の global bounds を可視化しやすい
- panel outside をマスクできる
- panel 作成 preview を page 座標で一貫して描ける
- dirty rect も page-space で統一しやすい

編集コマンド自体は panel local を維持し、desktop input 側で page → active panel local 変換を行う。

## 追加した回帰防止

- compositor で `PanelSurface` の global bounds を尊重することを確認する回帰テストを追加
- draw / fill dirty rect を page-space に変換するテストを追加
- `CreatePanel` コマンドで任意矩形 panel を relayout なしに追加するテストを追加

## 今後の設計指針

同種の不整合を避けるため、描画 surface / dirty rect / input mapping について次を守る。

1. **surface は必ず「どの座標系の矩形か」を型または API 契約で明記する**
2. consumer は source image の寸法と destination rect を勝手に同一視しない
3. dirty rect は local-space のまま上位へ返さず、境界を跨ぐ層で明示的に変換する
4. panel local 編集と page-space presentation を混同しない

## 追加で検討すべきこと

- `PanelSurface` に「local pixels + global rect」であることをより強く示す命名へ変更する
- compositor 側の blit API を、scale 版と non-scale 版でより明確に分離する
- dirty rect の生成元ごとに座標系を enum で持つ案も検討できる

今回の修正で、少なくとも panel move に伴う edge stretching / stale redraw は、症状ではなく原因層で是正できる。
