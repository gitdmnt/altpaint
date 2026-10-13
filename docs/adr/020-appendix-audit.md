# ADR 020 付録: コードベース監査と検証の記録、および運用記録

- 作業日時: 2026-09-23
- 作業エージェント: Claude Opus 5.5 (claude-opus-5-5)。調査・検証はサブエージェント群 (監査 9、検証 6、方針 6) で実施
- 対象: ブランチ refactor/ui-theme-and-clipping の HEAD af4ec88 (未コミットの ADR 019 作業は評価対象外)
- 付録 A〜F は ADR 020 の判断根拠である。行番号は af4ec88 時点のもの
- 付録 H「運用記録」は Phase 0 以降に追記する (スパイクの計測値・スクリーンショット・判定結果、Phase 1 の期限の判定、再レビューの打ち切りの判定、8 章 2 の調整値と理由)

## A. 調査の方法

1. **監査 (静的)**: 8 領域 (ドメイン / ペイント / desktop 中核 / 提示 / パネル / 永続化 / 履歴・文書 / 横断) を並列に読み、最後に領域横断の完全性チェックを行った。問題は計 141 件 (critical 11 / high 52 / medium 56 / low 22)。
2. **再現 (動的)**: 隔離 worktree 2 つで失敗テストを書いて実行した。実 GPU (NVIDIA GeForce RTX 2070, Vulkan) で本番と同じ GPU 経路を通している。テストは未コミットのまま次に残っている。
   - `.claude/worktrees/wf_c4d31800-528-1` (ブランチ worktree-wf_c4d31800-528-1): `apps/desktop/src/app/tests/repro_gpu_paint.rs`
   - `.claude/worktrees/wf_c4d31800-528-2` (ブランチ worktree-wf_c4d31800-528-2): `apps/desktop/src/app/tests/repro_nongpu_tests.rs` ほか 3 ファイル
   - ビルドには本体の Cargo.lock のコピーと `PYTHON3` の指定が必要だった (Cargo.lock 未追跡のため、新規解決では blitz alpha が新版になりビルド不能)。
3. **反証 (静的)**: 構造上の主張 22 件を 4 人の検証者がコードと git で数え直し、誇張を補正した。
4. **方針**: 3 立場 (作り直し / その場で差し替え / 止血して機能を出す) の案を独立に作り、2 観点 (オーナー価値 / 再発防止) で審査して統合した。

## B. 再現テストの結果

| ID | 結果 | 内容 | 観測値 |
|---|---|---|---|
| R1 | reproduced | ストローク後に AddRasterLayer / RemoveActiveLayer / MoveLayer を実行すると、command_effects が ActiveKomaLayers 同期を行い、古い CPU bitmap から GPU テクスチャを再構築するため、描いた画素が消える | 失敗メッセージ「AddRasterLayer 後に元レイヤーの GPU テクスチャからストロークが消えた (dark 画素数 654 -> 0)」。AddRasterLayer+RemoveActiveLayer でも 654 -> 0 だった。前提の確認として、ストローク直後の GPU テクスチャ中心は [0,0,0,255]、CPU bitmap は [255,255,255,255] (dark=0) だった。CPU 経路の対照テストは pass した。 |
| R2 | reproduced | ストロークのあとに AddKoma または CreateKoma を実行すると、Full 同期で全コマが CPU から再アップロードされ、既存コマの描画が消える | CreateKoma: 「CreateKoma 後に既存コマの GPU ストロークが消えた (dark 画素数 654 -> 0)」。AddKoma: 「AddKoma 後に既存コマの GPU テクスチャから描画が全て消えた (dark 654 -> 0, CPU 側 dark=0)」。 |
| R3 | reproduced | 重なる 2 本のストロークを描いて undo を 1 回すると、1 本目の重なり部分も消える (undo の before を古い CPU bitmap から取っているため) | 「undo 1 回で 1 本目の画素が壊れた: 交差点 dark=false (値 [255, 255, 255, 255]), 1 本目直後との差分画素数=81, dark 数 first=906 second=1731 undo=825」。差分の 81 画素は、ペン幅 9 に対応する 9x9 の交差部分。CPU 経路の対照テストは pass した (差分 0)。 |
| R4 | reproduced | ストロークのあとに PNG export するとストロークが含まれない。スナップショット作成→復元でもストロークが正しく往復しない | export: 「PNG export にストロークが含まれない: 画素 [255, 255, 255, 255], dark 数=0 (512x512)」。snapshot: 「スナップショット往復が不正: snapshot 文書に A=false, 復元後 GPU: A=true B=true (B は消えるべき), 復元後 CPU: A=false B=false」。 |
| R5 | reproduced | text.render_to_layer で描いたテキストが GPU テクスチャに反映されない | 「テキストが GPU テクスチャに反映されない: CPU dark=2144 GPU dark=0」。テストは TEXT_RENDER_TO_LAYER サービスを経由し、描画後に prepare_present_frame を 1 回実行している。 |
| R6 | reproduced | タップ (Down と Up が同じ座標) では履歴が閉じない。そのため次のストロークと 1 つの履歴に統合され、undo 1 回で両方が消える | CPU と GPU の両方で「undo 1 回の結果が不正: タップ点 dark=false (残るべき), ストローク dark=false (消えるべき)」。単独ストロークの場合は「単独ストロークを undo できない: execute_undo=false, undo 後も dark=true」。Up を最後の Drag と同じ点にした 2 本のケースでも「1 本目 dark=false (残るべき)」だった。 |
| R8 | reproduced | ストローク後に新規文書作成やプロジェクト読込をしても履歴が消去されず、KomaId(1) と layer_index が再利用されるため、undo/redo が新文書の画素を書き換える | 3 本とも「文書差し替え後も旧文書の履歴が残り新文書へ適用された: can_undo=false can_redo=true redo 実行=true, 新文書 dark 数 0 -> 654 (新 koma id=KomaId(1))」。GPU 版では GPU テクスチャ、CPU 版では CPU bitmap に、旧文書のストロークが書き込まれた。 |
| R15 | reproduced | 2 コマ以上のページで、ページ全面でないコマの GPU テクスチャが presenter でコマの bounds ではなくページ中央に描かれる | 条件: page=512x512、AddKoma 後のアクティブコマ bounds={x:260,y:20,w:232,h:472}、GPU テクスチャ=232x472、quad の destination=728x728、uv=0..1、bbox=[512,512]。失敗メッセージ「page 点 (270, 30) にはテクセル (10.5, 10.5) が出るべきが Some((130.5, 10.5))。shader 上のコマ表示原点=(140.0, 20.0), 期待 bounds 原点=(260, 20) |
| R7 | reproduced | document-model の AddKoma (グリッド自動再配置) で既存コマのレイヤービットマップが最近傍リサンプルされ、画素が不可逆に劣化する | 320x240 の 1 コマに 1px 市松模様を描いてから AddKoma した。1 件目: `assertion left == right failed: 既存コマのレイヤー寸法が AddKoma で変わった (リサンプルされた) left: (143, 216) right: (320, 240)`。2 件目: AddKoma → RemoveActiveKoma で 1 コマ全面へ戻した後、`AddKoma→RemoveActiveKoma の往復で 35328 / 76800 画素が変化した`。正しい  |
| R9 | reproduced | 起動時のプロジェクト読込失敗が握り潰され、空文書がそのパスに紐付くため、Ctrl+S (project.save_current) で読めなかった原稿ファイルが空文書で上書きされる | ゴミバイト列の場合: `読めなかった原稿ファイルが上書きされた: 起動後 project_path=...owner-manuscript.altp.json (起動パスと同一=true), 起動時 show_error 件数=0, 起動後コマ数=1, save_current 受理=true, 元 4096 bytes → 保存後 98304 bytes, 保存後に有効プロジェクトとして読める=true`。破損 SQLite (有効なプロジェクトを 1024 bytes に切り詰め、ヘッダーは残したもの) でも同 |
| R10 | reproduced | project-store の保存は既存ファイルを先に削除してから作り直す非アトミック方式で、保存が途中で失敗すると既存ファイルが失われる | 正常なプロジェクトを保存した後、同じパスへ PageId が重複したドキュメントを保存した (pages の PRIMARY KEY 違反で失敗させる注入)。保存は Err になったが、`保存失敗後に既存ファイルが書き換わった: before=69632 bytes, after=69632 bytes, reload=Err(invalid project file: missing metadata key: format_version)` となった。バイト長は同じだが中身は空スキーマに置き換わっており、元の |
| R11 | reproduced | ユーザーデータディレクトリ (dirs::data_dir()/altpaint) が無い環境で session / workspace preset / canvas size preset の保存が失敗する (本番コードに create_dir_all が無い) | 関数単位のテスト: `データディレクトリ不在で保存が失敗した (4/4)`。内訳は save_session_state / save_workspace_preset_catalog / save_canvas_size_presets がいずれも「指定されたパスが見つかりません。 (os error 3)」、project_store::save_project_to_path が「sqlite failed: unable to open database file」。初回起動を模したテスト: stderr  |
| R12a | reproduced | view-controls / tool-settings のスライダー (altp:slider) がポインタ位置の値をホストへ運べない | ポインタは event_loop/pointer.rs と同じ順序 (Blitz 転送 → app 処理) で送った。拡大率スライダーを 50% から 80% 位置へドラッグすると `zoom が 1 → 0.25 (期待: > 1.0)` となり、位置に関係なく最小値 25% に落ちる。PanelRuntime レベルで Blitz に Down/Move/Up を送った後の Activate では `actions=[RequestService(ServiceRequest { name: "view.set |
| R12b | reproduced | text-flow のテキスト入力 (altp:input) がホストへ値を運べない | 入力欄をクリックし、キー H,E,L,L,O を event_loop/keyboard.rs と同じ dispatch_keyboard_shortcut 経路で送ってから描画ボタンを押した。結果は `テキスト入力→描画でキャンバスに何も描かれない (非白画素 0)。参考: 入力中のキーで active_tool が Pen → Eraser`。対照テスト (SetText を直接注入し、同じ描画ボタンをクリック) は PASS したので、ボタン経路は正常であり、失敗は入力値が届かないことによる。 |
| R12c | partially-reproduced | select (altp:select) の選択値がホストへ届かない | テストは書いていない。select をクリックしても、R12a と同じく payload `{}` の Activate しか発行されない。layers の set_blend_mode (layers/src/lib.rs:155-158) は `payload.value.is_empty()` のとき即 return するため、要求は発行されない。 |
| R12d | reproduced | layers / koma-list の render_action_list が生成する <li> (id 無し) をクリックしても選択要求が発行されない (hit テーブル構築で element_id の無い hit が捨てられる) | layers: `<li> (element_id=None) クリックでレイヤー選択されない: active_layer_index 1 → 1; hit テーブル結果=None; クリック前後のキャンバス非白画素 0 → 16`。koma-list: `active_koma_index 1 → 1; hit テーブル結果=None; ... 0 → 0`。collect_panel_hits には data_action=altp:activate:select_* / data_args={"id":..} |
| R13 | reproduced | features/text の Font8x8Renderer で日本語 ("あいう") を描くと画素が 0 になる | `"あいう" の描画結果の不透明画素が 0 (48x16)。比較: "abc" は 252 画素`。さらに render_text_to_bitmap_edit は `Some(BitmapEdit 96x32) が返ったが不透明画素は 0 (見えない描画を成功扱い)` となる。 |
| R16 | reproduced (コード読解で確定。テストは書いていない) | ウィンドウの CloseRequested で、未保存確認も保存ジョブの待機も無く即終了する | event_loop.rs:223 は `WindowEvent::CloseRequested => event_loop.exit(),` で、確認ダイアログも処理も無い。ApplicationHandler 実装に exiting / Drop による後始末は無く、DesktopApp に未保存 (dirty) 状態の追跡も無い (grep で該当なし)。main.rs:23-24 は run_app から戻るとそのまま return する。 |

補足: GPU テストの一部 (`features/paint/backend/tests.rs` の golden 2 件、`crates/gpu-paint/src/tests.rs` の一部) は catch_unwind で assert 失敗を握りつぶすため、GPU があっても失敗しない。GPU 不在時にパス扱いになるテストは 26 件。

## C. 反証レビューの結果

| ID | 判定 | 重大度 | 補正後の記述 (要約) |
|---|---|---|---|
| S1 | partially-confirmed | medium | 主張の中身はほぼ正確。DesktopApp のフィールドは 15 個 (pub(crate) 11、private 4: koma_gesture / status_bar / invalidation / background_jobs)。impl DesktopApp は 29 ブロック / 28 ファイル (mod.rs に 2 ブロック)。メソッドは 150 個で、うち #[cfg(test)] が 8、本番用が 142。ServiceHandler の型は主張どおりで、10 個のハンドラが登録されている。features 同士の import は json_store の 3 件だけ。app→features の参照は app/tests を除いて 29 行 (含めると 34 行)。ずれているのは features→app で、正しくは 16 行ではなく本番 14 行 (テストモ… |
| S2 | confirmed | high | 主張どおり。入口は約 10 系統ある: apply_document_command / apply_session_command / execute_service_request (ハンドラ 10 個) / dispatch_panel_event / dispatch_keyboard_shortcut / apply_paint_input + commit_stroke_to_history / execute_undo・redo (restore_koma_layer_region を直接呼ぶ) / load_project と snapshot_restore の document 直接代入 / 保存時の sync_gpu_bitmaps_to_cpu による document 上書き / パネルの drag・resize。3 経路の後処理は次のとおり食い違う。(1) lo… |
| S3 | confirmed | medium | Document は `pub session: EditorSession` を持つ。document-model の外で `.session` に触れる本番コード (テストモジュールは除外) は 37 行: apps/desktop 26、paint-engine 5、panel-runtime 6。大半は読み取りで、書き込みは 5 箇所 (bootstrap.rs:80 の代入、project/service.rs:82 の mem::take、tools/service.rs:67/150/166 の merge・replace)。DocumentCommand 18 種のうちビューをリセットするのは 7 種 (AddKoma / CreateKoma / RemoveActiveKoma / SelectKoma / SelectNextKoma / SelectPrevious… |
| S4 | confirmed | medium | Document::apply は () を返す。document_command_effects は Noop 以外なら実際に変化したかに関係なく常に true を返すので、無変化の操作 (例: コマが 1 つのときの RemoveActiveKoma) でも Full の GPU 同期が走る。挙げられた 6 系統はすべて実在し、persist_session_state の手動呼び出しもちょうど 12 箇所。ただし GpuSyncGranularity は command_effects の中で使われる入れ子の分類なので、完全に独立しているのは実質 5 系統。逆に、主張に無い検出機構もある: PresentInvalidation の各フラグ、CachedCanvasViewGeometry のキー比較、set_workspace_panels_json の値比較、StatusBar … |
| S5 | confirmed | high | 主張どおり。LayerTextureStore のキーは HashMap<(KomaTextureId, usize)> で、マスクも同じ。SQLite の layers / layer_chunks テーブルは PRIMARY KEY (koma_id, layer_index)。BitmapPatch / GpuRegionPatch は koma_id + layer_index を持つ。LayerNodeId はコマごとに created_layer_count から採番され、どのコマも LayerNodeId(1) から始まるので作品内で一意でない。補足すると、SQLite は保存のたびにファイルを削除して全件書き直すうえ、layer.id を metadata_json に保存している。このため SQLite 側の index キーが実害を生む可能性は低い。実害が大きいのは履歴… |
| S16 | confirmed | high | 主張どおり。EditHistory が持つのは PaintPatch::{Cpu, Gpu} だけで、DEFAULT_HISTORY_CAPACITY = 50。本番コードは EditHistory::new() しか使わず、with_capacity はテストだけなので容量は固定。document-model / editor-state / paint-engine に構造操作の undo は無い。history.push は execute.rs:87,144 の 2 箇所だけ。clear_edit_history は snapshots/service.rs:40 からしか呼ばれず、history.clear() を直接呼ぶ本番コードも無い。その結果 NewDocumentSized、load_project、レイヤーやコマの構造変更の後にも古いパッチが残る。容量は件数で数えていて… |
| S22 | refuted | low | 4 要素すべてを持つスライスは 11 個中 0 個なので、CLAUDE.md の記述は事実と異なる。翻訳器は 0/11: 翻訳器はすべて crates/panel-runtime の translator_registry.rs / request_translation.rs にあり、features/ には 'translat' という文字列すら無い。サービスハンドラは 9/11 (panel_interaction と status_bar には無い)。テストは 10/11 (view のディレクトリには #[test] が無い)。サブ状態は次の 3 群に分かれる。(a) 型は feature に定義されているが、DesktopApp のフィールドとして保持されるもの 6 個: workspace (WorkspaceState)、koma_gesture (KomaGesture、… |
| S6 | partially-confirmed | critical | レイヤー寸法 = コマ寸法は事実。Koma::new_blank がコマ寸法でレイヤーを作り、add_raster_layer は composite_cache の寸法を使う。AddKoma に加えて RemoveActiveKoma も、ページ内の全コマを既定グリッドへ並べ直し、全レイヤーとマスクを最近傍でリサイズする。CreateKoma (ドラッグ作成) は並べ直さないが、その後に AddKoma/Remove を実行すると手で配置したコマも潰される。構造操作には undo が無く、縮小→拡大を繰り返すと画質が不可逆に劣化する。この前提は gpu-paint (コマ単位のテクスチャと composite)、project-store (koma_id 単位の layers/layer_chunks)、履歴 (コマローカルの dirty)、presenter (アクティブコマのテクス… |
| S7 | confirmed | low | 事実どおり。presenter は redraw のたびに LoadOp::Clear で全レイヤーを描き直す。PresentFrameUpdate の 6 フィールドのうち、background/temp_overlay/ui_panel の各 dirty rect と canvas_transform_changed の 4 つはテストだけが読む。canvas_dirty_rect は本番で到達しない CPU 経路の upload_region にしか使われず、canvas_updated は profiler 計測にしか使われない。mark_status_dirty の本番呼び出しはちょうど 20 箇所。needs_status_refresh は present.rs:135 と :164-166 でリセットされるだけで、読む箇所は無い。ステータスバーはフラグと無関係に毎フレーム描… |
| S10 | partially-confirmed | high | 1 枚 = 11,845,142 px × 4 B = 47,380,568 B ≒ 45.19 MiB。3 レイヤー 1 コマでは、CPU がレイヤー 3 + composite_cache 1、GPU がレイヤーテクスチャ 3 + composite 1 で、計 8 枚 ≒ 361.5 MiB/ページ。CpuCanvasSnapshot (ページ寸法 1 枚) はアクティブページに 1 つだけなので、407 MiB はアクティブページだけの値で、追加ページは 1 枚あたり約 362 MiB。GPU 本番ではこのスナップショットを幅・高さの取得にしか使っていない。スナップショットは Document 全体の clone なので 1 件 ≒ 180.8 MiB/ページ、上限 20 件 (≒3.5 GiB/ページ)。さらに、保存時・PNG 書き出し時にも Document 全体を一時的に c… |
| S13 | partially-confirmed | high | 本番で CPU フォールバックに到達しないのは事実。presenter の初期化に失敗すると event_loop.exit() で終了し、成功時は install_gpu_resources が必ず gpu=Some にする。Rgba8Unorm storage 非対応のアダプタでは、終了ではなく panic の恐れがある。desktop の #[test] は作業ツリーで 208 件 (HEAD は 205 件)。このうち DesktopApp を構築するのは 109 件で、GPU を install するのは gpu_tests の 9 件だけなので、約 100 件のアプリ統合テストが gpu=None (CPU バックエンド・CPU 表示) で動く。残り約 100 件は経路に依存しない純粋な単体テストで、『大半が CPU 経路を検証』はやや誇張。GPU が無いと skip される … |
| S17 | confirmed | critical | 前半は事実。表示幾何はページ寸法から計算する (cpu_canvas_snapshot の幅・高さ → bbox_size = ページ寸法)。一方、GPU ソースはコマ寸法のテクスチャで、fs_main は中心原点のページ px をテクスチャの中心原点へ 1:1 で写す。このため、コマのテクスチャは bounds.x/y を無視してページ中央に等倍で描かれる。入力は bounds 基準でコマローカル座標へ変換されるので、全面 1 コマ以外 (AddKoma のグリッドや CreateKoma) では描いた位置と表示位置がずれる。CPU 経路はページバッファの bounds 位置へ貼るので正しく、テストはこちらしか通らない。後半も事実。キャッシュキーはテクスチャの実体を見ない。sync_koma_layers はレイヤーテクスチャを毎回作り直すため、単一レイヤー (Single) のコマで … |
| S18 | confirmed | high | 事実どおり。CursorMoved と Touch の f64 座標は `as i32` で 0 方向へ切り捨てられる。ページ座標も floor で usize になり、スタンプ位置も KomaLocalPoint(usize) へ round される。既定の 1280×800 ウィンドウでは初期の fit 倍率が約 0.17 で、1 画面 px ≒ 5〜6 ページ px に量子化され、細線がギザつく。StrokeSegment は 1 つの pressure を持ち、半径はセグメントごとに 1 回だけ決まる。マウスの筆圧は常に 1.0。MAX_STAMP_STEPS=64 でスタンプ数が打ち切られ、spacing は最小 1px。このため Fine Liner (base_size 2) で 1 イベントに約 128 ページ px 以上動くと、スタンプ間隔が直径を超えて点線になる。加えて… |
| S19 | confirmed | high | 事実どおり。brush pass はポインタのイベント (セグメント) ごとに、レイヤー全面 (既定ページで 11.8M スレッド) を dispatch する。各スレッドが最大 65 スタンプとの距離をループ計算するので、1 イベントあたり約 7.7 億回の距離計算になる。対照的に再合成パスは dirty 範囲に限定されている。flood fill はマスクを 1 反復 1px ずつ広げる方式 (Jacobi) で、上限は min(8192, w+h+4) (既定ページで 6991)。測地的な経路長が上限を超える蛇行形状では塗り残しが出るが、戻り値の FloodFillOutcome は呼び出し側で捨てられ、通知は無い。32 反復ごとに poll(Wait) で UI スレッドを止め、1 反復がレイヤー全面の dispatch なので、広い領域の塗りでは数千回の全面パスが同期で走る。保存… |
| S8 | partially-confirmed | medium | パネル基盤 8 系統の Rust は 13,942 行で、追跡中の全 .rs 45,003 行の 31.0%（未追跡の panel_theme_css.rs 208 行を含めると 14,150/45,211 = 31.3%）。cargo tree -e normal で desktop の依存グラフは 382 パッケージ（desktop 自身を含む。除くと 381）。panel-runtime のサブツリーは 357（panel-runtime 自身を含む）。panel-runtime 経由でしか入らない依存は 259 ではなく 248（--prune panel-runtime で 134 に減る。build エッジも含めると 403→147 で 256）。外部パネルをロードする仕組みは無く、同梱 12 個を並べた const 固定リストだけを読む。meta.json に権限のフィールド… |
| S8b | confirmed | low | TranslatedRequest{Document,Session,Service} と HostRequest{DispatchDocumentCommand,DispatchSessionCommand,RequestService} は同じ 3 経路で、html_wasm_panel.rs で 1 対 1 に詰め替えているだけ。RequestDescriptor と ServiceRequest はどちらも {name: String, payload: Map<String,Value>} で、フィールドを 1 つずつコピーしている。tool.* は panel-runtime の translator で SessionCommand に変換する。一方 view.* は ServiceRequest のまま desktop に渡し、features/view/service.r… |
| C10 | partially-confirmed | critical | Blitz に転送されるのはポインタの Down/Up/Move だけで、ホスト側の hit テーブルから出るイベントは空 payload の Activate だけ。SetValue/SetText/DragValue は定義されているが、本番コードで生成する箇所は無い（テストで直接投入しているだけ）。IME は捨てられ、パネル上のホイールも握り潰される。唯一の例外はグローバルショートカットで、キー名（shortcut/key 文字列）は keyboard ハンドラを持つ app-actions と tool-palette に届く。ただしこれはフォーカスとは無関係な一斉配信で、テキスト入力欄に文字を打ち込むことはできない。数え直すと、スライダー 18（color-palette 9、text-flow 3、tool-settings 2、view-controls 4）、テキスト/数値… |
| S20 | confirmed | high | state_set_* は HandlerEffects.state_patch に積むだけで、state_get_* はハンドラ開始時点の HostCallInput.state のスナップショットを読む。パッチはハンドラが終わってから html_wasm_panel.rs で適用されるので、同じハンドラ内では自分の書き込みが読めない。SDK 側にもキャッシュは無い。実害は color-palette にとどまらない。color-palette では set_mode_* の直後の render_dom が古い color_mode を読み、表示セクションとボタン状態が 1 操作遅れる（次の再描画は color セクションが変わったときだけ）。app-actions では show_new_form が SHOW_NEW=true を積んだ直後の render_dom が false を… |
| S14 | confirmed | low | docs/SKETCH.md:231 には『MVP ではまず本体モジュールとして実装し、API の境界が固まってから順次分離する』とあり、この行は init コミット 113df3c（2026-03-09）から存在する。同じ日の 03:50〜04:00 にネイティブの crates/builtin-plugins として 6 パネルを実装し、07:21 の c1ea458 で全 6 パネルを .altp-panel DSL + Wasm へ移した。init から約 6 時間後なので『初日から』は正しい。UI 方式を正確に並べると 4 段階・3 回の転換になる: ネイティブ Rust（03-09）→ DSL+PanelTree+Wasm（03-09）→ DSL→HTML 翻訳（ADR 009、04-26。前日の ADR 007 で Blitz の HTML 実験層を導入）→ HTML+CSS… |
| S15 | confirmed | medium | ローカルの Cargo.lock では blitz-dom/blitz-html/blitz-paint/blitz-traits/stylo_taffy が 0.3.0-alpha.2 に解決されている。マニフェストの指定は 0.3.0-alpha（プレリリースへのキャレット指定）。Cargo.lock は .gitignore:3 で除外されていて、コミットされたことが一度も無い。.wasm も .gitignore:5 で除外されており、追跡中の .wasm は 0 件。ソースツリーには 12 個、計 51,331,960 バイト（48.95 MiB）の .wasm があり、target/wasm32-unknown-unknown/debug の成果物とバイト単位で一致するので debug ビルドと確定した。desktop のテストは DesktopApp::with_option… |
| C7 | confirmed | critical | 主張どおり。save_project_to_sqlite_path は既存ファイルを先に削除し (remove_file)、Connection::open → initialize_schema (autocommit の execute_batch) → transaction の順で書く。一時ファイルへの書き込みと rename は無い。データ INSERT はトランザクション内だが、元ファイルは開始時点で消えているので、クラッシュや強制終了が起きると『スキーマだけの空 DB』が残る。この DB は format_version を持たないので次回の読込で失敗し、C8 の経路 (空ドキュメント) に入る。CloseRequested は event_loop.exit() だけを呼ぶ。未保存を追跡するフラグ自体がコードに存在しないため確認ダイアログは出せず、background_jo… |
| C8 | confirmed | critical | 主張どおり。bootstrap_state は load_project_from_path(...).ok() の後に unwrap_or_default() で既定 Document (1 ページ 1 コマの空白) を作る。エラーは eprintln もダイアログも出さず完全に捨てられる。読めなかった project_path はそのまま ProjectPaths に入る。Ctrl+S → PROJECT_SAVE_CURRENT → save_project_to_current_path → enqueue_save_project(self.paths.project_path) と進み、C7 の remove_file で元ファイルを削除したうえで空ドキュメントを書く。読込失敗の原因は、ファイル欠落 (初回起動なので正当) と、破損・スキーマ非互換 (危険) を区別していない。… |
| C9 | partially-confirmed | high | create_dir_all (create_dir を含む) は本番コードに 0 件で、5 箇所すべてが #[cfg(test)] モジュール内にある。user_data_dir() は dirs::data_dir()/altpaint を返すだけで、ディレクトリを作らない。実機でも %APPDATA%\altpaint は存在しない。BL-113 (2026-06-13 21:54) で CWD 相対から dirs ベースへ移行したが、リポジトリ直下の altpaint-session.json の最終更新は 06-13 20:13 で、移行後にこの環境で永続化が成功した痕跡は無い。失敗時の通知は経路によって違い、『eprintln のみ』は一部だけ正しい。eprintln のみなのは session 保存 (persist_session_… |
| S9 | partially-confirmed | medium | 構造の指摘はすべて正しく、数値だけ補正が要る。(1) UI 状態 (WorkspaceUiState = workspace_layout + panel_configs) は project metadata・session・workspace preset catalog の 3 箇所に保存される。起動時は既定プリセット → project → session の順に上書きするので session が勝つ。Open 時は project の ui_state で置き換え、直後に session を上書き保存するので project が勝つ。さらに、起動時は空の layout/configs を適用しないが、Open 時は無条件に置き換えるという差もある。(2) session JSON は DesktopSessionState { last_project_path, ui_stat… |
| S11 | partially-confirmed | high | 数値はおおむね実測と一致した。細部の補正は以下。(a) HEAD は 385 コミット。接頭辞の厳密一致で refactor 232、大小文字を無視すると 241。feat 66、fix 7、docs 33、test 2、chore 2、Merge 3、接頭辞なし 31。fix 7 件のうち faf35ee (AGENTS.md) と 0f3b5d7 (Phase 12 の構造変更) は製品のバグ修正ではないので、実質は 5 件。(b) ユーザー可視機能を増やしたコミットは、自分で一覧を判定して約 24〜27 件。03-09〜03-16 に集中しており (43bed60, 2ac5684, a323d73, 594bae4, 17929d2, eaf0235, 3decc17, 8d688d4, c85b32b, 2ad12f9, 1a64b73, c260156, 522d1ba, 77… |
| S12 | confirmed | high | SKETCH.md『MVPでやる』節の 11 項目をコードで判定した結果は次のとおり。【実装済み: 4】(1) 単一作品の管理: Work→Page→Koma→RasterLayer と SQLite の保存/読込/別名保存がある。ただし C7/C8 の安全性問題を抱える。(4) ラスタレイヤー描画: 追加/削除/改名/移動/合成モード/表示/簡易マスク。不透明度は無い。(5) 基本ブラシ: ペン/消しゴム/塗りつぶし/投げ縄塗り、筆圧、AA、手ブレ補正。(8) UIプロファイル切り替え: workspace preset の適用/保存/書き出し/再読込。【部分的: 6】(2) ページ作成とコマ分割: コマの追加 (グリッド自動配置)・矩形ジェスチャでの作成・削除・選択はある。ページを追加・切り替える DocumentCommand も service も無く、ページ作成は未着手。しかも … |
| S21 | confirmed | high | 主張どおり。SKETCH.md は 59e61fa (2026-06-14, B10) で冒頭に『アーカイブ（初期構想）』の注記が付き、『歴史記録として保持』と明記された。代わりに正本とされたのは ARCHITECTURE.md / MODULE_DEPENDENCIES.md で、どちらも構造の文書であり要件や優先順位は持たない。ROADMAP.md (153 行) の中身は完了フェーズの表 (Phase 13 で止まり、B0〜B10 は載っていない)・維持項目 (テスト・計測・文書同期といったプロセス)・完了済み項目・Phase 9 の完了記録だけで、今後の機能項目は 0 件。しかも ADR 018 は、タイルキャッシュ・hit 収集の dirty スキップ・レイテンシ実測・プラグイン属性機構を『ROADMAP へ』送ると書いているのに、ROADMAP には 1 件も載っていない。do… |

## D. 領域別の監査結果

各領域の判定 (salvage) と問題一覧。重大度は監査時点のもので、C の反証で補正されたものがある。

### ドメインモデルと編集状態 (crates/document-model, crates/editor-state, crates/geometry, crates/raster)

- 判定: **rewrite-module**
- 判定の理由: document-model の製品コードは約 1.4k 行と小さく、作り直すこと自体は安価です。本当のコストは、ピクセル正本・ID・履歴・永続化キーの前提が周辺 (gpu-paint のストア、project-store のスキーマ、panel-runtime の host_state、desktop の約 35 箇所の session 直接参照) に浸透している点です。ここを少しずつ直すと、「レイヤー寸法 = コマ寸法」「(koma_id, layer_index) キー」「Document が画素とセッションを持つ」という 3 つの前提を個別に剥がすたびに、全周辺を触ることになります。そこで document-model はドメインモデル v2 として作り直し (集約ルート分離、全体一意 ID、PixelStore ハンドル、ChangeSet を返す純関数 apply、ページ単位のレイヤーツリー、構造化テキスト、コマ単位の版管理)、周辺をアダプタ経由で段階的に付け替えることを推奨します。editor-state はカタログ・上書き値・選択状態への分割による refactor-in-place で足ります。geometry は Rect<Space> への統合で refactor-in-place、raster は blend を残してラスタライザを削除する形で refactor-in-place です。リポジトリ全体を捨てる根拠は、この領域にはありません。捨てるべきは、モデルの前提を固定している部分です。
- 要約: ドメイン層の製品コードは約 3.0k 行と小さく、依存も健全です (wgpu/winit/wasmtime 非依存、geometry はローカル依存ゼロ)。ブレンド式の単一実装、ToolDescriptor・view_policy といった純粋関数、座標 newtype など、部品単位では再利用できるものが多くあります。一方で構造には 3 つの問題があります。 第一に、ピクセルの正本が決まっていません。Document が CPU の RgbaBitmap と composite_cache を持ちますが、本番の GPU 経路は CPU を一切更新しません。そのため、レイヤー追加・コマ追加時の GPU 再同期、undo の before 取得、PNG export、スナップショット、テキスト描画のどれもが古い CPU 画素を参照します (静的解析からの推定で、実機での再現は未確認)。 第二に、Document が EditorSession を内包しています。文書は「二大入口」として分離済みとしていますが、実態は名目上の分離です。DocumentCommand がビューをリセットし、新規作成でツールや色が初期化され、スナップショットを復元するとセッションまで巻き戻ります。 第三に、モデルが SKETCH の核心要件を表現できません。コマは usize の AABB でページ内にクランプされ、レイヤー寸法はコマ寸法と同じ、種別はラスタのみです。ページ全体の合成、テキストオブジェクト、コマ単位スナップショット、ブランチ、ページ追加はどれもありません。 加えて、ID は index …

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| domain#0 | critical | state-ownership | ピクセルの正本が CPU Document と GPU テクスチャに二重化し、同期が場当たり的 | apps/desktop/src/features/paint/backend/gpu.rs:8-11 「Paint Runtime は GPU 経路でも CPU bitmap を変更しない」、:180-186 commit_stroke は before を CPU bitmap から取得 / apps/desktop/src/features/paint/backend/tests.rs:249 gpu_apply_does_not_touch_cpu_pixels (CPU を更新しないことを仕様として固定) |
| domain#1 | high | correctness | 履歴が (KomaId, layer_index) をキーにし、構造変更・ロード・新規作成と整合しない | apps/desktop/src/features/paint/history.rs:18-28 と 36-44 (BitmapPatch / GpuRegionPatch は koma_id と layer_index をキーに持つ)、:69-74 (PaintPatch は画素差分のみ) / 履歴をクリアするのは apps/desktop/src/features/snapshots/service.rs:40 の 1 箇所だけ (features/paint/mod.rs:27)。features/project/service.rs:73-100 の load_project はクリア |
| domain#2 | high | boundary | Document が EditorSession を内包し、「作品とセッションの分離」が名目上に留まる | crates/document-model/src/document.rs:85-98 (Document.session: EditorSession)、:537-539 focus_active_koma_view → session.reset_view、:627-640 (SelectKoma などの DocumentCommand がビューをリセット)、:642-644 NewDocumentSized が `*self = Document::new` でセッションも初期化、:651-653 / crates/document-model/Cargo.toml:9 (editor- |
| domain#3 | high | requirements-gap | ドメインモデルが SKETCH の核心要件 (コマ中心・テキスト・スナップショット/ブランチ・ページ) を表現できない | crates/document-model/src/document.rs:121-134 (Page は width/height だけで、断ち切り・ノド・セーフエリア・見開きがない)、:147-154 (KomaBounds は usize の AABB)、:716-735 (コマをページ内へ強制クランプ) / crates/document-model/src/document.rs:222-241 と 289-300 (Koma.layers は RasterLayer のみ。ラスタ以外のレイヤー種別もページ単位のレイヤーもない) |
| domain#4 | high | correctness | コマを追加・削除すると、全コマの画素が最近傍法で破壊的にリサンプルされる | crates/document-model/src/document.rs:484-503 add_koma → relayout_page_komas、:522-535 remove_active_koma → relayout_page_komas / crates/document-model/src/document.rs:708-714 relayout_page_komas (全コマの bounds をグリッドで上書き)、:737-753 resize_koma_to_bounds、:755-801 resize_bitmap_nearest / resize_mask_neare |
| domain#5 | medium | abstraction-mismatch | index 参照と安定 ID が混在し、暗黙の「アクティブ」前提に依存する | crates/document-model/src/command.rs:21-35 (SelectLayer と MoveLayer は id 指定)、:49 SelectKoma { index }、:15-47 RemoveActiveLayer / RenameActiveLayer / ToggleActiveLayerVisibility など暗黙のアクティブ対象 / crates/document-model/src/document/layer_ops.rs:185-189 apply_bitmap_edits_to_koma_layer が active_layer_inde |
| domain#6 | medium | complexity | 不変条件を型で保証せず、事後補正と後方互換の残骸で維持している | crates/document-model/src/document.rs:545-572 normalize_after_load (ページ・コマ補完、index の clamp、再レイアウト) / crates/document-model/src/document/layer_ops.rs:348-369 ensure_koma_layers (空レイヤー列を composite_cache から復元する、レイヤー導入前の旧 bitmap 時代の移行処理)。本番から 10 箇所で呼ばれる |
| domain#7 | medium | architecture | コマンド適用が命令型の &mut で、変化情報を返さないため、副作用の知識が二重化している | crates/document-model/src/document.rs:578-646 (Document::apply(&mut self) は `()` を返し、失敗・no-op・影響範囲を返さない) / apps/desktop/src/app/command_effects.rs:19-86 (コマンド種別ごとに GpuSyncGranularity と UI 同期を desktop 側で推測する表) |
| domain#8 | medium | performance | 大きな画素バッファの深いコピーと多重保持、タイル化の欠如 | crates/raster/src/bitmap.rs:13-21 (RgbaBitmap は Vec<u8> で共有も COW もない) / crates/document-model/src/document.rs:230-231 (派生データの composite_cache をドメインに保持) |
| domain#9 | medium | correctness | PageDirtyRect をページ・コマローカル・任意ビットマップ座標に共用し、座標の型区別が破綻している | crates/geometry/src/coordinates.rs:272-279 (PageDirtyRect の doc は「ページ座標系」) / apps/desktop/src/features/paint/backend/mod.rs:38-39 (「コマローカル dirty rect」なのに型は PageDirtyRect)、features/paint/history.rs:21-22,40-41 も同様 |
| domain#10 | medium | dead-or-vestigial | raster のラスタライザが本番で未使用、テストが所有者と違う場所にあり、未実現のフィールドが残る | crates/raster/src/bitmap.rs:54-597 (draw_* / erase_* / paint_capsule 系の約 540 行)。消費者はテストだけ (rg: project-store/src/project_file.rs:102 のテストと document-model/tests.rs) / crates/paint-engine/src/ops/stamp.rs:17-56 (本番の CPU ブラシは別実装の generated_round_stamp で、重複している) |
| domain#11 | medium | correctness | 合成のアルファ式が非標準で、CPU と GPU に手動で複製されている | crates/raster/src/blend.rs:84-112 composite_pixel (dst の色を dst alpha で重み付けしない)。doc コメント :88-91 は「premultiply 風の挙動」と認めている / crates/document-model/src/document/tests.rs:900-905 (半透明 (0,255,0,128) を透明地に置くと (0,128,0,128) になり、色が半減することをゴールデンとして固定) |
| domain#12 | medium | state-ownership | EditorSession がディスク由来のカタログ、ユーザーの調整値、一過性の選択状態を混在させている | crates/editor-state/src/session.rs:420-455 (tool_catalog と pen_presets を active_* や view_transform と同じ構造体に持ち、すべて session JSON へ保存) / crates/editor-state/src/session.rs:570-586 (筆圧・AA・手ぶれ補正の変更がカタログのエントリを直接書き換える。ペンサイズだけは active_pen_size という別経路) |
| domain#13 | low | docs-drift | 文書と実装の乖離 (現況・目標・コード内 doc) | crates/editor-state/src/view_policy.rs:1-8 (「B5 で editor-state へ移設予定」とあり、存在しない `crate::document::Document::apply_session_command` を参照) / docs/IMPLEMENTATION_STATUS.md:243-263 (「`app-core` には次がある」: 削除済みのクレート)、:420-437 (render・canvas・DSL 前提の「強い点/途中の点」) |
| domain#14 | high | process | ドメインが初週のプロトタイプのまま凍結され、インフラのリファクタが先行した (プロセス上の根本原因) | git log -- crates/app-core/src/document.rs: 機能 (feat) コミットは 2026-03-09〜2026-03-14 だけ (レイヤー・スナップショット・テキスト・ツールカタログ)。以降の約 25 件はすべて refactor・改名・GPU 移行 / docs/ROADMAP.md:15 (Phase 9A「CPU フォールバック削除」) と docs/adr/018-naming-and-vertical-slice-rearchitecture.md:55,90-92 (BL-136「CpuPaintBackend を維持」): 方針が反転して |

再利用できる部品:
- geometry の座標 newtype 群 (WindowPoint / PagePoint / PagePointF / KomaLocalPoint / PanelSurfacePoint) と KomaBounds の座標変換メソッド。phantom 型の Rect<Space> に統合して再利用する
- raster::blend の BlendMode と合成式、および CPU/GPU 対応表とゴールデンテスト群。アルファ規約を見直すときの差分検出基準として使う
- raster::RgbaBitmap::extract_region / pixel_rgba / set_pixel_rgba などの最小バッファ操作。タイルの内部表現として流用できる
- editor-state の view_policy、ToolDescriptor (resolve_size / gesture_kind / size_policy)、per_tool_sizes のサイズ記憶ロジックと、それぞれのテスト
- document-model の with_layers_mut / edit_layers_region パターン (変異後に派生データを自動更新する入口)
- parse_document_size、default_koma_grid_bounds (破壊的な再レイアウトではなく「コマ割り提案」の純関数として再利用)
- DocumentCommand / SessionCommand の語彙一覧 (新コマンド体系の初期リスト、I/O 排除の方針)、および BL-148 の安定 ID 指定の方向性

### ペイント/GPU 描画パイプライン (crates/paint-engine, crates/gpu-paint + WGSL, crates/canvas-geometry, apps/desktop/src/features/paint)

- 判定: **rewrite-module**
- 判定の理由: 致命的な問題は個々の関数の出来ではなく、4 つの不変条件の欠落にある: 「画素の正本」「ストロークのライフサイクル」「履歴対象の同一性 (index 参照)」「ブラシのデータモデル (整数・セグメント単位)」。これらは paint-engine / features/paint / document-model / presenter の境界を横断している。致命バグの止血 (直前同期の追加、GPU 上での before 取得、明示的な StrokeEnd、dispatch 範囲の限定) は S〜M で可能なので、まず止血を先行すべきである。ただし、SKETCH の筆圧ブラシ、CSP ブラシ読込、100 ページ規模、コマンド単位でメモリ上限ベースの Undo に到達するには、画素ストア (タイル・ID キー・単一所有) とストロークモデル (f32 サンプル → ダブ → ストロークバッファ) の再設計が避けられない。その場の小修正を重ねると、GPU 化時と同じ「横付け」を繰り返す危険が高い。一方、GPU カーネル、GPU→GPU スナップショット、view 幾何、gesture 状態機械、ToolDescriptor は健全で、新設計へ移植できる。約 8,000 行のうち 4,000〜5,000 行程度は再利用可能と見積もる。この領域を全廃棄する必要はない。
- 要約: この領域は、個々の部品を見ると筋が良い。PaintPlan による純データ計画、クロージャ注入型の gesture 状態機械、純粋な canvas-geometry、document-model から切り離された gpu-paint カーネル群がそれに当たる。しかし部品間の不変条件がどこにも定義されていないため、ユーザーデータを失う致命的な欠陥が複数ある。最大の問題は画素の正本が二重化していることである。GPU 経路は Document の CPU ビットマップを一切更新しない (ADR 005)。それにもかかわらず、レイヤー追加・削除・移動やコマ追加時の GPU 再構築、PNG 書き出し、スナップショット、テキスト描画、Undo の before 取得は CPU ビットマップを正本として読み書きする。その結果、未保存ストロークの消失、古い書き出し、Undo による他ストロークやレイヤー全体の巻き戻りがコード上で成立している。ストローク確定がポインタ Up 時の Paint 出力に便乗しているため、タップや静止リリースでは履歴が閉じず、次のストロークと合体したり別レイヤーへ誤適用されたりする。CPU/GPU 二重実装は、ペン先・流量・背景消去・スタンプ累積・0.5px の中心ずれで意味論が乖離している。そのうえ本番では GPU が無条件に導入され、CPU フォールバックは到達不能である。テストは主に CPU 経路を通り、GPU テストはパニックを skip 扱いにするため、本番経路の欠陥を検出できない。ブラシのデータモデルも薄い。整数座標が入力から GPU まで通り、筆圧はセグ…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| paint#0 | critical | state-ownership | 画素の正本が二重化し、GPU 経路の編集が Document へ戻らない (未保存ストロークの消失・古い書き出し) | apps/desktop/src/features/paint/backend/gpu.rs:74-169 GpuPaintBackend::apply は document を読むだけで CPU ビットマップを更新しない (backend/tests.rs:248-375 がこれを要件として固定している) / apps/desktop/src/app/services/gpu_sync.rs:12-48 GPU→CPU 同期は保存時のみ (呼び出し元は background_tasks.rs:39 だけ) |
| paint#1 | critical | correctness | Undo の before スナップショットを stale な CPU から取っており、Undo で他ストロークやレイヤー全体が巻き戻る | apps/desktop/src/features/paint/backend/gpu.rs:177-186 commit_stroke は document.capture_koma_layer_region (CPU) を before にする / apps/desktop/src/features/paint/backend/gpu.rs:204-208 apply_fill も CPU から before を取る。flood fill の dirty はレイヤー全域 (crates/paint-engine/src/plan.rs:93-110) |
| paint#2 | high | correctness | ストローク確定が「Up 時に Paint 更新が出たとき」だけで、タップや静止リリースで履歴が合体・誤適用される | apps/desktop/src/app/input.rs:204-210 commit_stroke_to_history の唯一の呼び出し点が CanvasGestureUpdate::Paint かつ action==Up の分岐 / crates/paint-engine/src/gesture.rs:163-176 Up 位置が last_position と同一なら CanvasGestureUpdate::None を返す (Down→Up のクリックや、止めてから離す操作) |
| paint#3 | high | architecture | CPU/GPU 二重実装の意味論が乖離し、CPU フォールバックは本番で到達不能 (テストは CPU 経路ばかり) | apps/desktop/src/event_loop.rs:170-183 presenter 初期化に失敗すると exit、成功すれば無条件に install_gpu_resources する。apps/desktop/src/presenter/mod.rs:143-157 Rgba8Unorm storage 非対応はログを出すだけ ("compute pipeline may panic") / docs/adr/018-naming-and-vertical-slice-rearchitecture.md:55,90-92 「GPU 初期化失敗時のフォールバックとして維持」と do |
| paint#4 | high | correctness | GPU ブラシの被覆が plan.dirty からはみ出す (偶数サイズで右端・下端 1px) | crates/gpu-paint/src/shaders/brush_stroke.wgsl:35 center = pos + 0.5、:39 cov = radius + 0.5 - d / crates/paint-engine/src/ops/stroke.rs:63-75 dirty は point - size/2 から size 幅。stamp.rs:103-104 の CPU スタンプ中心は radius - 0.5 |
| paint#5 | high | correctness | flood fill の反復上限 (w+h+4) で黙って塗り残し、UI スレッドを同期ブロックする | crates/gpu-paint/src/fill.rs:175 max_iter = min(8192, w + h + 4)。非凸・渦巻き状の領域で測地距離が w+h を超えると部分塗りのまま終了し、エラーも出ない / crates/gpu-paint/src/fill.rs:193-242 1 反復ごとにレイヤー全面を dispatch し、32 反復ごとに device.poll(Wait) で UI スレッドが同期待ちする |
| paint#6 | high | requirements-gap | ブラシエンジンのデータモデルが薄く、CSP 系ブラシ要件に耐えない (整数座標・区間一定の筆圧・スタンプ上限) | apps/desktop/src/event_loop.rs:231,237 winit の f64 座標を as i32 で切り捨てる / crates/canvas-geometry/src/view_geometry.rs:147-153 map_view_to_canvas が floor して PagePoint(usize) を返す。crates/geometry/src/coordinates.rs:151-154,203-206 PagePoint/KomaLocalPoint は usize。ops/stroke.rs:34-37 スタンプ座標を round する |
| paint#7 | high | architecture | Undo/Redo がペイント専用・レイヤー index 参照・件数上限で、要件 (コマンド単位・メモリ上限) へ拡張できない | apps/desktop/src/features/paint/history.rs:17-44 BitmapPatch/GpuRegionPatch は layer_index で対象を指す (crates/document-model/src/document.rs:291 に RasterLayer.id があるのに未使用) / apps/desktop/src/app/command_effects.rs:44-49 MoveLayer/RemoveActiveLayer 後も履歴をクリアしない (clear は snapshots/service.rs:40 のみ)。古いパッチが別レ |
| paint#8 | high | performance | GPU ブラシがセグメントごとにレイヤー全面 × 全スタンプを走査する | crates/gpu-paint/src/brush.rs:173-182 layer_width/8 × layer_height/8 のワークグループで dispatch し、plan.dirty を使わない / crates/gpu-paint/src/shaders/brush_stroke.wgsl:34-44 全画素で最大 65 スタンプのループを回す |
| paint#9 | medium | correctness | straight alpha で保存しながら premultiplied 風の合成式を使い、透明レイヤー上の AA 縁・半透明色が暗くなる | crates/raster/src/blend.rs:91-113 composite_pixel の doc 自体が「透明地に半透明 src を置くと色値が src alpha 倍へ縮む (premultiply 風) 挙動」を認めている / crates/gpu-paint/src/shaders/brush_stroke.wgsl:50-54、fill_apply.wgsl:32-37、layer_composite.wgsl:52-58 も同じ式 |
| paint#10 | medium | performance | GPU 資源の常駐・確保方針が 100 ページ作品に耐えない | apps/desktop/src/app/mod.rs:249-263 sync_all_layers_to_gpu が全ページ・全コマ・全レイヤーとコマごとの composite を VRAM に常駐させ、追い出しが無い。Document も同じ画素を RAM に保持する (二重保持) / 既定ページで 1 レイヤー約 47MB。100 ページ × (2 レイヤー + composite) で約 14GB (推定) |
| paint#11 | medium | boundary | 表示幾何の寸法源と座標系が GPU テクスチャの実寸と一致せず、コマ配置・ページ表示を扱えない | apps/desktop/src/app/canvas_state.rs:57-62 canvas_dimensions は CPU スナップショット (ページ寸法: cpu_canvas_snapshot.rs:21-22) 由来 / apps/desktop/src/app/present_api.rs:147-165 GPU ソースはアクティブコマのテクスチャ (コマ寸法) |
| paint#12 | medium | abstraction-mismatch | PaintBackend 抽象が名目的で、PaintPlan が自己完結していない (プラグイン時代の残骸を含む) | apps/desktop/src/features/paint/backend/cpu.rs:51-58 CPU は _plan を無視し、input から compute_paint_edits で context を再解決する / apps/desktop/src/features/paint/backend/gpu.rs:75-81 GPU は _input を無視する。gpu.rs:100-106 で opacity/antialias を session から再読込する (PaintPlan に含まれない) |
| paint#13 | medium | correctness | コマ外へはみ出したストロークがコマ境界にクランプされ、境界沿いに線が引かれる | apps/desktop/src/app/input.rs:170-175 描画中の点を active_koma_bounds.clamp_canvas_point でクランプする / crates/document-model/src/document.rs:181-190 canvas_to_koma_local は境界外で None を返す |
| paint#14 | medium | test | テストが GPU 本番経路の欠陥を検出できない構造になっている | crates/gpu-paint/src/tests.rs:163-168 と apps/desktop/src/features/paint/backend/tests.rs:230-237 は、catch_unwind の Err (= パニック。wgpu 検証エラーを含む) を「非対応環境」として skip する / crates/gpu-paint/src/tests.rs:18-44 GPU デバイス取得に失敗すると、テストは静かに pass する |
| paint#15 | low | docs-drift | 文書・ADR がコードと食い違い、GPU 移行の中途痕跡とデッドコードが残る | CLAUDE.md:76 と docs/MODULE_DEPENDENCIES.md:153,687 の canvas-geometry::map_view_to_canvas_with_transform は存在しない (実体は crates/canvas-geometry/src/view_geometry.rs:130 map_view_to_canvas) / docs/RENDERING-ENGINE.md:45,284「編集中の CPU→GPU 転送禁止」に対し、backend/gpu.rs:60-64 はストローク commit のたびに before を upload する ( |

再利用できる部品:
- crates/gpu-paint のシェーダとディスパッチ骨格 (layer_composite.wgsl、lasso_fill_mark.wgsl、fill_apply.wgsl、composite.rs の dirty 限定 dispatch)。alpha 式と dispatch 範囲を修正のうえ流用
- crates/gpu-paint/src/gpu/snapshot.rs の snapshot_region / restore_region (タイル CoW 型 Undo の基礎)
- crates/gpu-paint/src/gpu/readback.rs の行パディング解除 readback
- crates/gpu-paint/src/pipeline.rs の compute パイプライン生成ヘルパと、uniform バイトレイアウトテストの手法
- crates/canvas-geometry の写像ロジックとテスト (f32 化と 3x3 行列化をして流用)
- crates/paint-engine/src/gesture.rs の状態機械骨格 (Begin/End イベントを追加して流用)
- crates/paint-engine/src/plan.rs の「画素なし純データ計画と dirty の事前確定」という設計
- crates/editor-state/src/tool_descriptor.rs の ToolDescriptor
- apps/desktop/src/features/paint/history.rs の EditHistory スタック (対象 ID 化・容量バイト化)
- apps/desktop/src/app/invalidation.rs の GpuSyncGranularity のような宣言的な副作用分類
- crates/paint-engine/src/ops/mod.rs の point_in_polygon 等 (GPU 仕様に揃えたテストオラクルとして)

### デスクトップ composition root と制御フロー (apps/desktop/src の app/・event_loop/・features/ のうち paint 以外)

- 判定: **rewrite-module**
- 判定の理由: この領域の核、つまり DesktopApp の状態・効果モデル (app/mod.rs、invalidation.rs、command_router/effects、services/registry、host_request_router、background_tasks、panel_config_sync、bootstrap の後処理) は、入口の多重化・後処理の手書き・画素の所在管理の欠如という構造的な欠陥を中心に抱えている。その場の小さな修正を重ねても、ほぼ全メソッドに手を入れることになる。そこで、この核の 3〜4 千行を新しい AppState / AppCommand / ChangeSet / EffectRunner / PixelStore / JobRunner に置き換えるモジュール単位の作り直しを推奨する。一方で、葉のロジック (ジェスチャ、リサイズ計算、ローダ、パス解決、入力の正規化、presenter との境界) は純粋関数や明確な型としてよくできており、テストのシナリオも移植できる。したがって全面破棄は不要である。進め方は同じクレート内の strangler 方式とする。まず PixelStore を導入して問題 1 の実害を止め、次に ServiceHandler を &mut DesktopApp から切り離し、入口を AppCommand へ 1 つずつ寄せる。
- 要約: DesktopApp は文書上「composition root に縮小済み」とされているが、実態は 15 フィールド・約 150 メソッドを 28 ファイルの `impl DesktopApp` に分散させた God object のままである。features/ の各スライスは `fn(&mut DesktopApp, &ServiceRequest)` を受ける部分 impl にすぎない。スライス間の結合は import ではなく self.* のメソッド名前空間を通って隠れている。状態変更の入口は DocumentCommand、SessionCommand、ServiceRequest、PanelEvent、直接のペイント経路、document の直接代入など 10 系統に分かれている。各入口が invalidation の後処理を手書きしているため、load / new / snapshot restore のように同じ意味の操作でも後処理が食い違う。最も深刻なのは画素の正本が GPU テクスチャと CPU の Document に二重化している点である。本番は常に GPU モードで、ストローク中は CPU 画素を更新しない。それにもかかわらず、レイヤー追加・コマ作成・エクスポート・スナップショット・テキスト描画は CPU を正本とみなして読み書きする。コード読解の範囲では、未保存画素の消失や古い画素の書き出しが起きる (実行は未検証)。提示用の dirty 管理は CPU 合成時代の層別 dirty rect が温存されたもので、計算されても使われないフィールドや…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| desktop-core#0 | critical | correctness | 画素の正本が GPU テクスチャと CPU Document に二重化し、所在管理がないため GPU モードで画素消失・古い画素の書き出しが起きる | apps/desktop/src/features/paint/backend/gpu.rs:9,177-185 GPU 経路は CPU bitmap を変更しない設計。commit_stroke の before も古い CPU から取る / apps/desktop/src/features/paint/backend/tests.rs:361-363 GPU fill が CPU 画素を変えないことをテストで固定している |
| desktop-core#1 | high | state-ownership | 状態変更の入口が 10 系統に分かれ、各入口が後処理を手書きしているため同じ意味の操作でも後処理が食い違う | apps/desktop/src/app/command_router.rs:15-28 と app/command_effects.rs:19-143 DocumentCommand/SessionCommand の入口と種別 match / apps/desktop/src/app/services/registry.rs:18,23-34 ServiceHandler = fn(&mut DesktopApp,&ServiceRequest)->Option<bool> の 10 ハンドラ |
| desktop-core#2 | high | architecture | DesktopApp は God object のままで、features/ は垂直スライスではなく `impl DesktopApp` の分割ファイルになっている | apps/desktop/src/app/mod.rs:113-137 フィールド 15 (pub(crate) 11)。PaintState 7・PresentInvalidation 11 フィールドを内包 / `impl DesktopApp` 29 ブロック / 28 ファイル、メソッド約 150 (app/invalidation.rs 23、app/input.rs 15、app/mod.rs 12、app/present_api.rs 11) |
| desktop-core#3 | high | correctness | 編集履歴が構造変化や文書差し替えで無効化されず、layer_index と KomaId(1) の再利用で undo が誤ったレイヤーに当たる | apps/desktop/src/features/paint/history.rs:19-45 BitmapPatch / GpuRegionPatch のキーが koma_id + layer_index (表示順 index) / clear_edit_history の呼出しは features/snapshots/service.rs:40 の 1 箇所だけ (load_project / NewDocumentSized / RemoveActiveLayer / MoveLayer / RemoveActiveKoma では消去しない) |
| desktop-core#4 | medium | dead-or-vestigial | 提示の無効化 (dirty) 管理の大半が CPU 合成時代の遺物で、計算されるが使われない | apps/desktop/src/app/invalidation.rs:24-32 PresentFrameUpdate の 6 フィールドのうち、app/frame.rs:138-193 の compose_frame が使うのは canvas_dirty_rect と canvas_updated だけ。background / temp_overlay / ui_panel_dirty_rect と canvas_transform_changed を読むのはテストだけ / apps/desktop/src/app/present.rs:164-166 `if needs_status |
| desktop-core#5 | high | requirements-gap | 非同期モデルが「thread::spawn して描画時にポーリング」だけで、重い処理の多くが UI スレッドで同期実行される | apps/desktop/src/app/background_tasks.rs:37-53 保存の前に sync_gpu_bitmaps_to_cpu (app/services/gpu_sync.rs:12-46 全コマ全レイヤーの同期 readback) と document.clone() を UI スレッドで実行する。既定の A4 350dpi 2894x4093 ではレイヤー 1 枚あたり約 47MB / apps/desktop/src/features/project/service.rs:73-75 load_project_from_path (SQLite + zstd  |
| desktop-core#6 | medium | duplication | wire 名から操作への翻訳が panel-runtime と desktop の 2 段に分かれ、同じ対応表が二重に書かれている | crates/panel-runtime/src/request_translation.rs:268-343 tool. と layer. は Session/DocumentCommand に翻訳し、それ以外の 10 名前空間は wire 名を known_names に列挙して ServiceRequest として素通しする / apps/desktop/src/features/view/service.rs:9-32 view.* を SessionCommand へ、features/koma/service.rs:9-31 koma_nav.* を DocumentCommand |
| desktop-core#7 | medium | architecture | 描画フェーズが状態を書き換え、入力処理が直前フレームの派生状態に時間的に依存している | apps/desktop/src/app/present.rs:24-34 prepare_present_frame の中で、poll_background_tasks (44)、layout 更新 (45-54)、パネル同期 (59-87)、update_panel_geometry による hit テーブルの書込み (240-316)、reconcile (105-111) を行う / apps/desktop/src/app/input.rs:23,279-291 入力処理は self.layout (present でのみ設定され、app/mod.rs:205 の初期値は None) |
| desktop-core#8 | medium | boundary | ホストからパネルへのデータ経路が 3 系統あり、永続 config が派生データの注入口として流用されている | apps/desktop/src/app/panel_config_sync.rs:12-42,44-81 キャンバスサイズプリセットとワークスペースプリセットの一覧を JSON 文字列にして、特定パネル (panel_ids::APP_ACTIONS / WORKSPACE_PRESETS) の persistent config に書き込む / apps/desktop/src/features/tools/service.rs:125-129 ペン import のレポートを TOOL_PALETTE の config に書き込む |
| desktop-core#9 | medium | performance | CpuCanvasSnapshot が GPU モードでも寸法の正本として必須で、構造操作のたびにページ全体の RGBA を再構築する | apps/desktop/src/app/canvas_state.rs:57-62 canvas_dimensions は cpu_canvas_snapshot の width/height を返す。app/present.rs:45-48 のレイアウト、app/invalidation.rs:333-341 の表示幾何、app/input.rs:316 の入力写像がこれに依存する / apps/desktop/src/app/cpu_canvas_snapshot.rs:1-5 に「GPU が標準経路だが (width, height) は依然参照される」とある。18-47 でページ全体 |
| desktop-core#10 | medium | test | テストが本番と異なる状態モデル (CPU) を検証しており、実装の詳細や常に通る条件に依存している | 本番は apps/desktop/src/event_loop.rs:171-183 で常に GPU を初期化する。一方 app/tests/interaction.rs:29-96 と event_loop/tests.rs:71,107 の画素検証は、install_gpu_resources なしで build_cpu_canvas_snapshot(&app.document) を読む → 問題 1 を原理的に検出できない。GPU テストは 10 件で、GPU がなければ skip する (gpu_tests.rs:57-59) / テストから内部フィールドを直接操作・参照している:  |
| desktop-core#11 | low | boundary | 入力層がパネルの内部構造やサービス名を知り、アプリの内部フィールドに直接触れている | apps/desktop/src/event_loop/keyboard.rs:60-68 Ctrl+N で activate_panel_control("builtin.app-actions", "app.new") と、パネル ID と DOM の node id をハードコードしている / event_loop/keyboard.rs:38-89 Ctrl+S / Ctrl+O / Alt+PageUp などで ServiceRequest を直接構築する → ショートカットの系統が 3 つある (panel_runtime.dispatch_keyboard のパネル側 regis |
| desktop-core#12 | low | state-ownership | EditorSession (ツール・ペンのカタログとビュー) が Document の中にあり、文書操作と一緒に巻き戻ったりコピーされたりする | crates/document-model/src/document.rs:95-97 Document.session: EditorSession。crates/editor-state/src/session.rs:431-436 で tool_catalog / pen_presets を保持する / apps/desktop/src/features/snapshots/service.rs:38 スナップショット復元でツール・色・ビュー・カタログまで巻き戻る。一方 features/project/service.rs:81-83 の読込では mem::take で退避しており、経 |
| desktop-core#13 | low | docs-drift | 文書がコードと食い違い、desktop の到達度を過大に評価している | docs/IMPLEMENTATION_STATUS.md:382-437,831-838 が app-core::Document / canvas::CanvasRuntime / ui-shell / render を現役として記述している / docs/adr/018-naming-and-vertical-slice-rearchitecture.md:198 の「god object を解消」と IMPLEMENTATION_STATUS.md:148 の「直接フィールド参照ゼロ」はコードと一致しない (問題 3、12) |
| desktop-core#14 | low | dead-or-vestigial | 小さな痕跡的要素が本番コードに散在している | apps/desktop/src/features/tools/catalog.rs:91,94 provider_plugin_id / drawing_plugin_id を必須として検証するが、PaintPlugin の registry は R5 で撤去済みで、UI 表示 (crates/builtin-panels/tool-palette/src/lib.rs:97-98) 以外に用途がない / apps/desktop/src/main.rs:30-34 と app/bootstrap.rs:40-41,264-275 で session を 2 回読み込み、起動時のパス解決ロジ |

再利用できる部品:
- features/koma/gesture.rs (純粋な状態機械とテスト)
- features/panel_interaction/state.rs の compute_resized_rect と Drag/Resize/Press の状態型
- app/cursor.rs (リサイズハンドル → カーソルの写像)
- features/json_store.rs (Loaded/Missing/Corrupt の 3 状態ローダ)、features/workspace/workspace_presets.rs、features/project/session.rs、canvas_size_presets.rs の serde 型
- platform/ (DesktopDialogs ポート、paths の純関数、normalize_*_path)
- event_loop/pointer.rs のホイールアニメーションと筆圧正規化、event_loop/keyboard.rs のキー正規化
- app/frame.rs の ComposedFrame → PresentFrame (所有と借用の分離) と present_api の提示境界
- app/invalidation.rs の GpuSyncGranularity (効果をデータで宣言する発想。ChangeSet の原型)
- app/bootstrap.rs の preset → project → session の適用順序と meta 由来の既定配置生成
- features/text/raster.rs の TextRenderer trait の枠 (font8x8 実装は日本語や縦書きに対応しないため置き換えが必要)
- テスト: TestDialogs と DesktopAppOptions による隔離、振る舞い系のシナリオ (ポインタでの描画、コマ作成、セッション往復、プリセット適用、起動復元)

### 提示 (presenter) とフレーム合成 — apps/desktop/src/presenter/・present_quads/・app/{present,frame,present_api,invalidation,cpu_canvas_snapshot,canvas_state}.rs、および panel-html の GPU 描画 (gpu.rs / view/present.rs) と panel-runtime の render_panels との接点

- 判定: **rewrite-module**
- 判定の理由: この領域の問題は GPU 部品ではなく、糊層 (app/present.rs・frame.rs・present_api.rs・invalidation.rs の提示部分・cpu_canvas_snapshot.rs・present_quads の DesktopLayout / CanvasPlan / LayerDirtyAccumulator、約 1.3k 行) の状態所有・幾何・同期モデルに集中しています。糊層は小さく、テストが死んだ意味論 (dirty rect、CPU スナップショット) を固定しているため、段階的に解きほぐすより、FrameContext (DisplayMetrics + 1 回だけ計算した表示幾何) → FrameScene (配置付きキャンバス面の列 + PanelPlacement の列 + オーバーレイ) → presenter という新しい骨格で書き直すほうが安くて安全です。GPU パイプライン、SDF シェーダ、quad ビルダ、canvas-geometry、panel-html の GPU ターゲットは健全なので流用します。したがって、この領域を理由にアプリ全体を作り直す必要はありません。先に GpuContext と Surface を分離して golden テストの足場を作り、その上で糊層を差し替える順序を推奨します。
- 要約: GPU 部品そのものは小さく健全です。単一の wgpu Device/Queue を gpu-paint・presenter・vello で共有しており、レイヤーテクスチャやパネルテクスチャを CPU readback なしで 1 レンダーパスに合成できています。QuadPipeline への統合や、純関数の quad ビルダも保守しやすい形です。問題の大半は GPU 部品の周囲にある「フレーム構築の糊層」(app/present.rs・frame.rs・present_api.rs・invalidation.rs・cpu_canvas_snapshot.rs、約 1.2k 行) にあります。この層は CPU 合成時代 (ADR 010 以前) の dirty rect 機構と CPU スナップショットを残したまま GPU 経路へ付け替えられました。dirty rect は GPU presenter からほぼ消費されておらず (毎フレーム LoadOp::Clear で全描画)、キャンバス寸法の正本も「ページ大の CPU RGBA バッファ」という偶有的な経路に残っています。この構造から、コード読解上は次の正しさの問題が導かれます。(1) GPU 経路ではコマのテクスチャをページ寸法の幾何と中心基準シェーダで描くため、ページ全面でないコマは bounds 位置でなくページ中央に表示される。(2) presenter の bind group キャッシュが (koma_id, サイズ) キーのため、同サイズの新規作成や読込で旧テクスチャが表示され続ける。(3) 描画順 (登録順…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| presenter#0 | critical | correctness | GPU 経路のキャンバス表示モデルがコマ配置を表現できない (ページ全面でないコマの位置がずれる / アクティブコマしか表示されない) | apps/desktop/src/app/invalidation.rs:331-361 canvas_view_geometry は CpuCanvasSnapshot (ページ寸法) から TextureQuad を計算する / apps/desktop/src/app/present_api.rs:147-166 canvas_gpu_source_spec が返すのはコマ寸法のテクスチャ (koma.layers[0] / composite_cache のサイズ) |
| presenter#1 | high | state-ownership | GPU 常駐状態の同期が手続き的で、presenter キャッシュの無効化キーが不完全 (古い画の表示) | apps/desktop/src/presenter/frame.rs:544-550 canvas bind group の再生成判定は (koma_key, kind, layer_index, width, height) で、テクスチャ実体の同一性を見ない / crates/gpu-paint/src/gpu/store.rs:204-221 sync_koma_layers は毎回 clear_layers_for_koma でレイヤーテクスチャを作り直す。同一 KomaId(1)・同サイズの NewDocumentSized (document.rs:337-346, 642-64 |
| presenter#2 | high | dead-or-vestigial | dirty rect 差分提示の機構が GPU 化後は実質死んでいる (ADR 010 の名残) | apps/desktop/src/app/frame.rs:138-194 compose_frame が PresentFrameUpdate から使うのは canvas_dirty_rect (CPU フォールバックの転送範囲) と canvas_updated (profiler) だけ / apps/desktop/src/app/invalidation.rs:25-32 background / temp_overlay / ui_panel の dirty rect と canvas_transform_changed は本番では誰も読まず、テスト (app/tests/inte |
| presenter#3 | high | dead-or-vestigial | CpuCanvasSnapshot がキャンバス寸法の正本になっており、CPU フォールバックは本番で到達不能 | apps/desktop/src/app/cpu_canvas_snapshot.rs:18-48 ページ寸法 × 4 byte (A4 350dpi で約 47MB) を確保して白で埋め、アクティブコマを貼る / apps/desktop/src/app/invalidation.rs:151-157 invalidate_document_structure は GpuSyncGranularity::None (SelectLayer / SelectKoma 等、command_effects.rs:56-75) でも毎回 refresh_cpu_canvas_snapshot を呼ぶ |
| presenter#4 | high | test | 提示の正しさを検証するテストが存在せず、presenter を headless で構築できない | apps/desktop/src/presenter/mod.rs:101-111 WgpuPresenter::new(window: Arc<Window>) は Surface 生成と Device 生成が一体で、オフスクリーン描画ができない / compose_frame / WgpuPresenter::render はテストから一度も呼ばれない (grep: 呼出は event_loop.rs:108,119 のみ) |
| presenter#5 | medium | abstraction-mismatch | キャンバス用の回転シェーダを汎用ブリッタとして流用している (サイズ不一致時は中央寄せの切り抜きになる) | apps/desktop/src/presenter/shaders.rs:117-157 fs_main は uv_min/max を bbox (metrics) のピクセル空間へ写し、textureDimensions で中心基準に戻す / apps/desktop/src/presenter/frame.rs:488-499 パネル/ステータスの quad は destination = screen_rect、bbox = screen_rect 寸法、uv 0..1 |
| presenter#6 | medium | duplication | キャンバス表示幾何が複数箇所で別の約束事により計算されている | apps/desktop/src/app/input.rs:27-35 (host 矩形は絶対座標) と input.rs:316-327 (host 矩形を (0,0) 基準へ置き換え + clamp 済みの点) で viewport の原点規約が異なる / apps/desktop/src/app/invalidation.rs:242-250 (変換前の幾何)、331-361 (キャッシュ付き)、present.rs:219-223 (キャッシュ温めのためだけの呼出) |
| presenter#7 | medium | correctness | パネル矩形の正本が 3 つあり、描画順とヒット判定順が一致しない | crates/panel-workspace/src/workspace.rs:159-175 panel_rect (workspace の entry.size と位置) は features/panel_interaction/state.rs:54-61 panel_rect_in_window を経て、アクティブ枠 (invalidation.rs:280-286) と dirty 計算に使われる / apps/desktop/src/app/present.rs:240-316 full_rect は View の panel_size (measured) から作られ、prese |
| presenter#8 | medium | complexity | フレーム DTO の多段詰め替えと、所有・借用の二重表現 | apps/desktop/src/app/frame.rs:27-62 ComposedFrame と CanvasSurfaceData が、presenter/frame.rs:56-146 の PresentFrame<'a> / CanvasSurface / CanvasSurfaceSource とほぼ同形で重複する (frame.rs:66-118 は変換だけのコード) / 借用型 TextureSource<'a> のために所有型を別に作っているが、CPU 経路は結局 pixels.clone() する (frame.rs:221) |
| presenter#9 | medium | architecture | GPU リソースの所有がウィンドウに結合し、デバイスロストや致命エラーの扱いが粗い | apps/desktop/src/presenter/mod.rs:101-141 Instance・Adapter・Device・Queue を presenter が Surface と一緒に生成する / Arc<Device> の保持者: presenter/mod.rs:49-52、gpu-paint store.rs:47 (ctx)、brush.rs:41-42、fill.rs:34-35、composite.rs:28-29、panel-runtime runtime.rs:37-42。app/mod.rs:232 では GpuCanvasContext が別インスタンスとしても |
| presenter#10 | medium | requirements-gap | HiDPI 非対応が多数の定数と呼出に直書きされている | scale_factor / ScaleFactorChanged の参照は 0 件 (grep)。event_loop.rs:159 は LogicalSize で窓を作るが、以降はすべて物理 px (event_loop.rs:106 inner_size、230-231 CursorMoved の物理座標) / scale 1.0 の直書き: present_api.rs:121 render_panels、app/present.rs:287-291 collect_panel_hits、features/status_bar/mod.rs:8「スケール 1.0 固定」 |
| presenter#11 | medium | correctness | presenter 自前描画の色が sRGB サーフェスで二重ガンマになり、テーマ色の定義元も分散している | apps/desktop/src/presenter/mod.rs:163-168 sRGB サーフェスを優先して選ぶ / presenter/pipelines/mod.rs:26-31 と shaders.rs:199-202 は solid quad に u8/255 の sRGB 値をそのまま出力し、mod.rs:210-215 の clear_color も同様。線形→sRGB 変換はコード全体に存在しない (grep で 0 件) |
| presenter#12 | low | requirements-gap | マルチウィンドウ / マルチビューの前提がハードコードされている | apps/desktop/src/event_loop.rs:31-32 window と presenter は単一の Option。218 行で他の WindowId を無視する / presenter/mod.rs:101 はウィンドウごとに Device を生成する構造で、2 枚目のウィンドウは別 Device になりテクスチャを共有できない |
| presenter#13 | low | performance | quad ごとに uniform buffer・bind group・draw call を作り、インスタンシングしていない | apps/desktop/src/presenter/pipelines/quad.rs:101-133 quad 1 個ごとに buffer と bind group を作り、pool は増える一方で縮まない / quad.rs:139-150 1 quad = set_bind_group + draw(0..6) |
| presenter#14 | low | performance | 変化の有無にかかわらず毎フレーム CPU で再計算している | apps/desktop/src/app/present.rs:102 → 240-316 refresh_panel_hit_tables が毎フレーム全可視パネルで collect_action_rects を実行する (panel-html view/actions.rs:38-47 の query_selector_all と、要素ごとに HashSet を確保する compute_absolute_position (130-146)) / app/present.rs:62 build_workspace_panels_json が毎フレーム serde_json で直列化する ( |
| presenter#15 | low | correctness | オーバーレイが軸平行矩形しか表現できず、画面端での bbox 計算にも誤りがある | crates/canvas-geometry/src/view_geometry.rs:136-177 map_source_rect_to_display は 4 隅の AABB を返すため、回転したビューではアクティブコマのマスク・枠・作成プレビューが外接矩形になる (overlay_quad.rs:162,172,197) / SolidQuad は WindowRect (usize) 固定 (solid_quad.rs:19-23) |
| presenter#16 | low | docs-drift | 提示まわりの文書・コメントが現況と乖離している | CLAUDE.md:76、docs/MODULE_DEPENDENCIES.md:153,687、docs/IMPLEMENTATION_STATUS.md:275 が存在しない関数 canvas-geometry::map_view_to_canvas_with_transform を正本の経路として記述している / docs/RENDERING-ENGINE.md:88 の「UI ベース層は CPU で生成する」、98 の存在しないパス apps/desktop/src/frame/overlay_quad.rs、125 と 244 の「dirty rect による差分更新は presen |

再利用できる部品:
- presenter/pipelines/quad.rs の QuadPipeline と QuadLayerRanges (インスタンシング化して流用)
- presenter/pipelines/present.rs の PresentPipeline 構築コード、および shaders.rs の SOLID / CIRCLE / LINE シェーダ (PRESENT_SHADER はキャンバス用と blit 用に分割して再利用)
- presenter/frame.rs の render の骨格 (単一パスで描き順を固定する構成、Surface の Lost/Outdated 処理、WIP の scissor_of)
- present_quads/solid_quad.rs・overlay_quad.rs の純関数ビルダと 17 件のテスト (入力 DTO だけ差し替え)
- crates/canvas-geometry (CanvasViewGeometry / TextureQuad と 9 件のテスト)。配置矩形と scale の対応を追加して表示幾何の単一正本にする
- crates/panel-html の PanelGpuTarget、on_render、resolve_action_rects / collect_action_rects (GPU 非依存の hit)
- panel-runtime の render_panels / collect_panel_hits の API 形 (Arc<wgpu::Texture> の所有受け渡し)
- gpu-paint の readback 群 (presenter の golden テストの読み戻しに転用)
- frame-profiler の計測点 (名前は GPU 経路の実態に合わせて整理)

### パネル/プラグインシステム (panel-runtime / panel-wasm-host / panel-protocol / panel-sdk / panel-macros / panel-html / panel-workspace / builtin-panels ×12 / docs/builtin-plugins)

- 判定: **rewrite-module**
- 判定の理由: アプリ全体の廃棄は不要ですが、パネル UI 層 (同梱パネルを Blitz HTML/CSS + Wasm DOM mutation で描く部分) はモジュール単位での作り直しを推奨します。理由は 3 つです。(1) 最も基本的な機能要件 (スライダー、テキスト入力、IME、select、スクロール、リスト選択) がコード上満たされていません。今の構造のまま直すには、キー/IME/ホイールの転送、DOM value の読み取り、Blitz イベントハンドラへの置換、select ポップアップの自作、z-order の導入、契約層の縮約、セクションと翻訳器の分散が必要で、L〜XL 規模の改修になります。(2) コスト (全コードの約36%、推移依存の93%、alpha 版エンジンの回避策、ビルド時間とバイナリの倍増) に対して、正当化するはずの拡張性の便益 (外部ロード、権限、容易な開発) はゼロです。(3) SKETCH:231 自身が「MVP では標準パネルは本体モジュール」と定めています。規模の目安は次のとおりです。現行は基盤 7 クレート約 11.9k 行 + パネル約 2.9k 行 + desktop 側グルー約 1.9k 行 ≒ 16.7k 行です。最初から設計する場合、egui (+egui-wgpu/egui-winit。wgpu/winit を共有でき、追加は数十クレート) によるホストネイティブ実装なら、12 パネルで約 1.5〜2.5k 行、typed Action の dispatch で約 0.3〜0.5k 行、panel-workspace の移植で約 1.2k 行 (または egui_dock) となり、合計約 3〜4.5k 行で入力・IME・スクロール・フォーカス・クリップ・DPI まで標準で揃います。将来の第三者パネル (MVP 要件の「プラグインパネル」) は、Wasm が型付きの宣言的 UI ツリーを返してホストが egui で描く方式 (SKETCH の「ホスト定義の中間表現」) にすれば、protocol 約 0.4k + ホスト側レンダラ約 0.8k + SDK 約 0.6k + 既存 panel-wasm-host 約 1.1k ≒ 3k 行で実現でき、権限リストと fuel/epoch 制限もこの時に設計します。Blitz を維持して正しく直す場合でも 7〜9k 行程度と重い依存が残ると見積もります (推測)。なお、egui は CSS のような自由なスタイリングや「HTML 知識の流用」を失う代わりに、SKETCH の「Web フロントエンド技術は採用しない」には合致します。移行は、代表パネル 2〜3 個 (tool-settings、layers) のスパイクで e2e の受け入れ条件を満たせるかを確認してから、パネル単位で段階的に行います。HTML を絶対条件にする場合の代替案は refactor-in-place で、優先順は、入力配線 → read-your-writes → リストの id と z-order → 死に経路の一括削除 → 契約の typed 化 → セクションと翻訳器の feature 分散 → 状態所有者の整理です。
- 要約: パネル/プラグインシステムは、コード全体の約36% (約16.7k行)、desktop の推移依存 382 クレートのうち 357 を占める最大の構造物です。一方、この構成を正当化するはずの SKETCH の要件 (Wasm パネルのランタイムロード、プラグイン開発の容易さ) は実現していません。ローダは同梱 12 個の固定リストで、外部ロード、権限モデル、fuel/epoch によるリソース制限はありません。つまり、コストだけを先に払っている状態です。最も重大なのは入力経路で、ホストからパネルへ届くのはポインタの Down/Up/Move と「DOM id の Activate」だけです。キーボード文字、IME、ホイール、スライダー値、select 値は一切届きません。コード上、スライダー 18 箇所、テキスト入力 7 箇所、select 4 箇所は値を運べず、`render_action_list` が生成するリスト項目は id が無いためクリックできません (いずれもコード読解による推定で、実機では未確認)。意味論レベルの不具合もあります。state patch が遅延適用されるため、ハンドラ内で書いた値を直後に読むと古い値が返ります。ヒットテストの順序が描画順と一致せず、Wasm トラップは記録されずに捨てられます。panel→host 方向の契約は 10 層以上に重なっています。同型の型が二重に存在し (TranslatedRequest と HostRequest、RequestDescriptor と ServiceRequest)、同種の操作でも翻訳箇所が一定し…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| panel-system#0 | critical | requirements-gap | パネルへの入力経路が欠落しており、スライダー・テキスト入力・select・スクロールが機能しない | apps/desktop/src/features/panel_interaction/state.rs:101-103,201-204 (ポインタから生成されるのは PanelEvent::Activate のみ。コメントでは SetValue 等を「Wasm が直接発行する経路に統一」としているが、Wasm は PanelEvent を発行できない) / crates/panel-runtime/src/html_wasm_panel.rs:350-353 (Activate は extra payload に json!({}) を渡す。DOM 要素の value を読まない) |
| panel-system#1 | high | architecture | 実現していない拡張性のために、同梱パネル全部を Wasm+HTML 経由に強制している | docs/SKETCH.md:231 (「標準パネルも将来的にプラグイン化…MVPではまず本体モジュールとして実装し、APIの境界が固まってから順次分離」) / crates/panel-runtime/src/loader.rs:61-110 (同梱 12 個の固定 const リスト。外部ディレクトリの走査は無い) |
| panel-system#2 | high | dependency | alpha 版ブラウザエンジンへの重依存 (推移依存の 93%、既知の不具合を多数回避中) | cargo tree: desktop 382 クレート中 panel-runtime 経由が 357、パネル基盤の外は 25 のみ (winit, rusqlite, dirs 等) / crates/panel-html/Cargo.toml:13-23 (blitz-* 0.3.0-alpha / taffy / vello / anyrender_vello) |
| panel-system#3 | high | correctness | id の無いリスト項目は hit テーブルから落ち、レイヤー/コマ一覧のクリック選択が不能 | crates/panel-sdk/src/dom.rs:287-309 (render_action_list は <li data-action=...> を生成するが id 属性を付けない) / apps/desktop/src/app/present.rs:296-300 (`let element_id = hit.element_id?;` で id の無い hit を破棄。HEAD 時点から同じ) |
| panel-system#4 | high | abstraction-mismatch | state patch の遅延適用により、ハンドラ内の read-after-write が古い値を返す | crates/panel-wasm-host/src/state_api.rs:283-286,368-370 (set_state_* は HandlerEffects.state_patch に積むだけ) / crates/panel-wasm-host/src/host_state_api.rs:31-40 (state_get_* は呼び出し前に clone した current_input.state を読む) |
| panel-system#5 | high | architecture | ホスト↔パネル契約が 10 層以上に重なり、同型の型の二重定義と経路の不統一がある | crates/panel-runtime/src/request_translation.rs:29-38 と host_request.rs:13-22 (TranslatedRequest{Document,Session,Service} と HostRequest{Dispatch…} は 1 対 1。html_wasm_panel.rs:253-271 で詰め替えるだけ) / crates/panel-protocol/src/lib.rs:122-136 と panel-runtime/src/services.rs:111-151 (RequestDescriptor と Ser |
| panel-system#6 | high | state-ownership | 状態の真実の所在が混線 (ホストのデータを panel config に保存、グローバルショートカットをパネルが所有、サイズとフォーカスを二重保持) | apps/desktop/src/app/panel_config_sync.rs:35-40,69-78 と features/tools/service.rs:125-128 (テンプレート一覧、ワークスペース一覧、ペン取込結果をパネルの persistent config に書き込み、session/project に保存させる) / apps/desktop/src/app/panel_config_sync.rs:150-159 と bootstrap.rs:64 (起動時に、選択中ワークスペースプリセットの正本をパネル config から読み戻す) |
| panel-system#7 | medium | performance | host state は JSON の中に JSON 文字列を入れ、毎呼び出しで全量 clone し、未使用の巨大ペイロードまで配っている | crates/panel-protocol/src/host_state.rs:94-97,118-122,188 (layers_json / komas_json / catalog_json / pen_presets_json / panels_json が JSON 文字列フィールドになっている) / crates/panel-runtime/src/host_state.rs:392-428 (pen_presets_json はチップビットマップ Vec<u8> を含む PenPreset 全体をシリアライズしているが、パネルでの使用は 0 件) |
| panel-system#8 | medium | test | ホスト側の json! 手組みとプロトコル DTO が二重定義で、両者を結ぶテストが無い | crates/panel-runtime/src/host_state.rs:340-362,392-428 (ホストは型の無い json! マクロで文字列キーを手組み) / crates/panel-protocol/src/host_state.rs:75-138 (DTO は Deserialize のみで serde(default) も無い) |
| panel-system#9 | medium | boundary | 水平なパネル基盤に feature 知識が集中したまま (ADR 018 の目標は未達) | crates/panel-runtime/Cargo.toml:9-11 (document-model / editor-state / raster に依存) / crates/panel-runtime/src/host_state.rs:100-118 (8 セクションすべてを runtime 内で登録。コメントは「feature 分散は B7」) |
| panel-system#10 | medium | correctness | ヒットテストの順序が描画順と異なる (z-order が無い) | crates/panel-workspace/src/lib.rs:36 (panel_geometries: BTreeMap<String, PanelGeometry>) / crates/panel-workspace/src/lib.rs:154-206 (move_handle/resize/hit をいずれも BTreeMap のキー順、つまり ID のアルファベット順で先勝ち) |
| panel-system#11 | medium | dead-or-vestigial | 方式変更の残骸と、使われていない経路が多数残存 | crates/panel-html/src/action.rs:58-67,123-149 (`altp:<kind>:` の kind 許可リストに未使用の "color" / "layer-select" があり、kind は検証後に捨てられる。ADR 012 は altp の解析経路を撤去したと記載) / crates/panel-runtime/src/html_wasm_panel.rs:403-417,433-438 (command: は "noop" のみ対応、service: はどの panel.html でも 0 件) |
| panel-system#12 | medium | correctness | エラーの黙殺 (Wasm トラップ、翻訳成功後の payload 欠落) | crates/panel-runtime/src/html_wasm_panel.rs:344-349,425-426 (dispatch_to_wasm(...).unwrap_or_default() → Wasm トラップを記録せずに捨てる) / crates/panel-runtime/src/html_wasm_panel.rs:309-317 (on_host_change が Err なら false を返すだけ) |
| panel-system#13 | medium | test | テストは件数の割に振る舞いを検証していない (native no-op 上のスモーク、空振りする e2e) | crates/panel-sdk/src/runtime/abi.rs:204-223,309-310 と dom.rs:79-83 (native では emit/DOM/state の API がすべて no-op か既定値) / crates/panel-sdk/src/test_macros.rs:327-335 と 12 パネルの assert_entrypoints! (呼べることしか確認しない) |
| panel-system#14 | medium | duplication | 12 パネルに定型コードと三重定義が散在 | panel_handler は 86 個で、大半が `fn undo() { emit_request(&services::history::undo()); }` 型の 1 行トランポリン (例: view-controls/src/lib.rs:98-126、app-actions/src/lib.rs:272-295) / payload 構造体の重複: SliderValue×4、TextValue×5、KeyEvent×2、`#[serde(crate = "panel_sdk::serde")]` が 17 箇所 |
| panel-system#15 | medium | process | 方式の迷走と、機能より構造を優先した完了判定 | git: 385 コミット中 183 がパネル系クレート (plugin-host / panel-dsl / panel-html-experiment / ui-shell 等の旧名を含む) に触れている / ADR 007 (04-25 HTML 実験、同日に第 4 稿まで改訂) → 009 (04-26 DSL→HTML 翻訳) → 011 (05-02 feature gate 撤去、パネルが実行時に消えていたことが判明) → 012 (05-03 DSL 撤去) → 013 (05-12 自動サイズを廃止して手動リサイズ) → 014 (05-15 PanelTree 撤去) →  |
| panel-system#16 | low | docs-drift | ドキュメントとコードの食い違い (撤去済み・単一権威・Engine 内完結などの主張) | docs/adr/013-html-panel-manual-resize.md:32 (サイズの単一権威) と apps/desktop/src/app/present.rs:254-282 (二重保持) / docs/adr/014-paneltree-removal-and-workspace-layout-html.md:86 (「Blitz が IME 入力を扱う」) と keyboard.rs:17-20 (IME を破棄) |

再利用できる部品:
- crates/panel-workspace 全体 (4 隅アンカー配置、8 ハンドルリサイズの純粋関数、PanelGeometry、WorkspaceLayout の永続化 DTO、meta 由来の既定配置)。z-order の追加だけで UI 技術に依存せず再利用できる
- crates/panel-protocol の names::* (操作語彙の棚卸し。typed Action enum 設計の一次資料)、keyboard.rs の正規化規約、apply_patches
- HostState section + revision キャッシュ + subscribes の考え方 (型付き view-model・差分購読として移植)。request_translation.rs の各 match 節にある payload 検証ロジックは typed Action のコンストラクタの素材になる
- crates/panel-wasm-host (DOM API を外した骨格)、crates/panel-macros、panel-sdk の typed payload と emit の骨格。将来の第三者パネル/フィルタ/I/O プラグイン用に凍結・縮小して保持する
- builtin-panels のドメインロジック: color-palette の HSV/Lab/Oklch 変換とテスト、tool-settings の対数スライダー変換、koma/layer のラベル整形関数
- panel.meta.json の宣言的 layout/preset/subscribes スキーマ (default_size と preset.size の重複は解消する)
- panel-html の GPU 直描画パターン (パネル毎の自前テクスチャ、Arc<Texture> の受け渡し、ヘッドレス resolve)。HTML を残す場合、または同様のテクスチャ合成に流用できる
- translator_registry の「未登録名を診断する」考え方と all_wire_names_are_registered 型の網羅テスト

### 永続化・入出力・設定 (project-store / pen-io / frame-profiler / desktop features: project・workspace・export・json_store・tools 読込 / platform: paths・dialogs / tools・pens データ)

- 判定: **rewrite-module**
- 判定の理由: 永続化領域の非テストコードは約 3,000 行と小さく、他領域との結合点は実質的に `Document`・`WorkspaceUiState`・`EditorSession` の 3 型だけです。そのため全体を壊す理由にはなりません。critical / high の正しさの問題（非アトミック保存、起動時ロード失敗の握り潰し、ディレクトリ未作成、export の stale）は、いずれも局所修正（S〜M）ですぐに塞げます。まずこれを行うべきです。一方で、要件（コマ単位スナップショット/ブランチ、差分保存、部分ロード、100 ページ）を満たすには、project-store のスキーマと保存戦略を作り直す必要があります。現スキーマは位置ベースのキー、全件の再作成、形骸化した版管理の上に立っているため、改修するより再設計した方が早いです。ただしこの再設計は、ドキュメントのメモリ常駐モデル（全ページ全ビットマップの所有）とピクセルの正本（GPU か CPU か）の決定に依存します。アプリ全体の方針決定を先行させる必要があります。チャンクコーデック、ラウンドトリップテスト、外部ブラシパーサ、3 状態ローダ、パス解決、ダイアログ抽象は保持価値が高く、全廃すると損です。
- 要約: この領域では、個々の部品は堅実です。具体的にはチャンクコーデック、ABR/GBR/SUT パーサ、JSON の 3 状態ローダ、パス解決の純関数を指します。一方で、それらを束ねる永続化オーケストレーション（いつ・何を・どこへ・どの版で書くか）は設計されていません。リファクタのたびに場当たり的に配線し直されてきました。最優先は正しさの問題で、原稿が消える経路が少なくとも 4 本あります。(1) 保存は既存ファイルを先に削除してから作り直す非アトミック方式で、終了時に保存ジョブを待たず、並行保存も防いでいない。(2) 起動時のロード失敗を握り潰し、空ドキュメントを旧パスに紐付けたままにするので、Ctrl+S で原稿が空で上書きされる。(3) BL-113 以降ユーザーデータディレクトリを作成しておらず、開発機でも %APPDATA%\altpaint が存在しない。(4) PNG export とスナップショットが GPU→CPU 同期を経ず、古いピクセルを書き出す。format_version は 2026-03-11 から 7 のまま、破壊的なスキーマ変更を 3 回通過して形骸化しており、移行戦略もありません。UI レイアウトとパネル config は project / session / workspace preset の 3 箇所に重複しています。優先順位も起動時と Open 時で異なり、ショートカットや派生データまで原稿ファイルに焼き込まれます。session は EditorSession を serde でそのまま書き出したもので、13.5MB あります。これを …

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| persistence#0 | critical | correctness | 保存が非アトミックで、原稿が消える経路が複数ある (先に削除→再作成・終了時に待たない・並行保存) | crates/project-store/src/project_sqlite.rs:203-206 既存ファイルを fs::remove_file してから Connection::open する / crates/project-store/src/project_sqlite.rs:209-211 initialize_schema がトランザクション外 (auto-commit) なので、途中で失敗するとスキーマだけの空 DB が残る。旧データはこの時点で削除済み |
| persistence#1 | critical | correctness | 起動時のロード失敗を握り潰し、空ドキュメントを旧パスに紐付けて上書き保存させる | apps/desktop/src/app/bootstrap.rs:41-46 load_project_from_path(&project_path).ok() → 失敗時は unwrap_or_default() の空 Document になり、ユーザーへの通知もない / apps/desktop/src/app/mod.rs:186-196 読めなかった bootstrap.project_path を ProjectPaths の現在パスとしてそのまま採用する |
| persistence#2 | high | correctness | ユーザーデータディレクトリを作成しないため、BL-113 以降は新規環境で session・preset・既定プロジェクトの保存がすべて失敗する | apps/desktop/src/platform/paths.rs:29-51 user_data_dir()/altpaint 配下のパスを返すだけで、ディレクトリを作らない / 本番コードに create_dir_all は 0 件 (grep で確認、テストコードにのみ存在) |
| persistence#3 | high | correctness | PNG export とスナップショット作成が GPU→CPU 同期を経ず、描いた内容が反映されない | apps/desktop/src/app/background_tasks.rs:37-40 保存は sync_gpu_bitmaps_to_cpu() の後に clone するが、56-62 の enqueue_export_png は同期なしで document.clone() する / apps/desktop/src/features/export/png.rs:45-51 CPU 側の koma.composite_cache をそのまま PNG にする |
| persistence#4 | high | process | format_version が形骸化し、スキーマ移行の戦略がない | crates/project-store/src/types.rs:10 CURRENT_PROJECT_FORMAT_VERSION = 7。git log -G で見ると、最後に値が変わったのは 55e4e8a (2026-03-11) / 090ec3b (B1) は panels→komas などテーブル・列・serde キーを「マイグレーションなし」で改名したが、version は据え置き。07544cb (B0) の列削除、b80d6a4 (B5) の document レコードからのセッション除去も同様 |
| persistence#5 | high | architecture | SQLite スキーマと保存戦略が要件 (差分保存・コマ単位スナップショット/ブランチ・部分ロード・100 ページ) に届いていない | docs/SKETCH.md:249-252 (コマ単位スナップショット・日時同期・枝・差分/フル切替・複数スナップショットのコマでページを構成)、274-279 (部分ロード・チャンク・差分/フル切替・破損復旧)、385-397 (100 ページの目標) / crates/project-store/src/project_sqlite.rs:32-38 ProjectSaveMode::Delta は列挙子だけで、保存処理 235-323 は常に全件 INSERT |
| persistence#6 | high | state-ownership | UI 状態の正本が 3 箇所に重複し、パネル config にユーザー設定と派生データが混在して原稿ファイルへ焼き込まれる | crates/panel-workspace/src/workspace_state.rs:18-25 WorkspaceUiState のコメントに「project / session の双方で共有」と明記されている / 書込み先は 3 箇所: project_sqlite.rs:228-232 (metadata.ui_state)、app/project_paths.rs:37-48 (session.ui_state)、features/workspace/service.rs:75-90 (preset catalog) |
| persistence#7 | high | boundary | session JSON が EditorSession をそのまま書き出したもので、起動時に上書きされるカタログまで含む 13.5MB を UI スレッドで高頻度に同期書込みする | apps/desktop/src/features/project/session.rs:20-29 editor_session: EditorSession を型ごと保存し、version フィールドがない。40-47 で to_vec_pretty + fs::write / crates/editor-state/src/session.rs:421-455 EditorSession は tool_catalog / pen_presets を含む。244-260 PenTipBitmap.data: Vec<u8> は JSON 数値配列になる |
| persistence#8 | medium | correctness | 破損ファイルへの耐性が低い: 不正なチャンクで panic し、起動不能ループになる恐れ。マスクは非圧縮 JSON | crates/project-store/src/project_sqlite.rs:995-1009 fill_chunk と 1011-1034 blit_chunk は、チャンク座標・寸法を bitmap の範囲と照合せずに index で書き込む。範囲外なら panic する (blit は payload 長しか検査しない) / project_sqlite.rs:844, 896 RgbaBitmap::transparent(width as usize, ...) は DB 由来の i64 を検証なしで usize 化する。負値なら巨大な確保になる (推測: abort) |
| persistence#9 | medium | correctness | JSON 設定の書込みが非アトミックで、「破損ファイルを温存する」保証が直後の保存で破られる | apps/desktop/src/features/json_store.rs:1-8, 25-52 load は Corrupt 時にファイルを温存すると説明している / しかし session.rs:40-47 の save は、次の persist_session_state (パネル操作ごとの高頻度トリガ) で既定値ベースの状態を上書きする。破損した session は、起動直後の最初のパネル操作で消える |
| persistence#10 | medium | performance | 保存・読込のコストが文書全体に比例し、重い処理が UI スレッドで走る | apps/desktop/src/app/background_tasks.rs:39 保存のたびに sync_gpu_bitmaps_to_cpu を UI スレッドで同期実行する。app/services/gpu_sync.rs:12-48 は全ページ全コマ全レイヤーを read_back_full し、レイヤーごとに restore_koma_layer_region → edit_layers_region でコマ全面を再合成する (document-model/src/document/layer_ops.rs:48-55) / background_tasks.rs:40 self |
| persistence#11 | medium | requirements-gap | pen-io の成果の大半が実行時に使われず、CSP ブラシ互換という要件への投資が回収されていない | pen-io は 2,398 行 (うちテスト約 550) / 主経路の GPU ブラシは tip を受け取らない: crates/gpu-paint/src/brush.rs:17-23 BrushStrokeParams は color / radius / opacity / antialias / mode だけ。tip を使うのは CPU フォールバックの crates/paint-engine/src/ops/stamp.rs:21-49 のみ |
| persistence#12 | medium | duplication | 変換層・ディレクトリ走査・ツール定義が重複し、ドメイン型の serde に直結しているため保存形式が脆い | collect_files が逐語的に重複している: crates/pen-io/src/fs_walk.rs:10-42 と apps/desktop/src/features/tools/catalog.rs:40-72 (BL-048 で共通化したものが、B7 のクレート分割で再び複製された) / ツール定義のコピーが 4 つある: tools/builtin/*.altp-tool.json (5 件)、features/tools/default_catalog.rs:14-71 (同じ 5 件)、editor-state/src/session.rs:346-401 の provi |
| persistence#13 | medium | dead-or-vestigial | 未使用 API・名残・実態と合わない名前が保存層に堆積している | 部分ロード API 3 本と ProjectManifest 系の 5 型が本番未使用 (crates/project-store/src/lib.rs:10-17、呼び出しは project_file.rs のテストのみ) / ProjectSaveMode::Delta / save_mode 列 / koma_composites.created_at_unix_ms は書き込まれるだけで、読んで使う側がない |
| persistence#14 | medium | test | テストが正常系のラウンドトリップに偏り、失敗系・実データ・テスト隔離に穴がある | project-store の 12 件 (project_file.rs:165-548) は良質だが、保存の中断・並行保存・破損チャンク・大きなビットマップ・KomaId の衝突は 0 件 / 実ファイル依存のテスト 3 件は、サンプルが無いと return で黙って成功する: pen_exchange/mod.rs:355-358, 378-381、pen_catalog.rs:170-172。サンプルは .gitignore:18-19 で除外されているため、CI や新規 clone では ABR / SUT の実データ検証がゼロ |
| persistence#15 | low | correctness | エラーの型が String に消去され、通知経路がなく、保存完了前に状態を確定してしまう | apps/desktop/src/app/background_tasks.rs:23, 44-46 ProjectStoreError を String 化し、JoinHandle<Result<(), String>> で返す / desktop の本番コードに eprintln! が 29 箇所あり、ロギング基盤 (log / tracing 依存) がない。session と canvas preset の保存失敗はユーザーから見えない (app/project_paths.rs:50-54, bootstrap.rs:165-167) |
| persistence#16 | low | process | リポジトリ直下に旧 CWD 時代のユーザーデータが残り、ignore 対象なのに追跡されてリファクタで編集されている | リポジトリ直下に altpaint-session.json (13.5MB, 2026-06-13)、altpaint-project.altp.json (旧スキーマ)、canvas-templates.json、workspace-presets.json がある / canvas-templates.json / workspace-presets.json は .gitignore で ignore 指定されているのに git ls-files で追跡中。B1 のコミット (8e93d61, 0a96ee9, d3e0ece) がパネル改名に追従してこれらを編集している |
| persistence#17 | low | abstraction-mismatch | frame-profiler を独立クレートにした理由が実現していない | crates/frame-profiler/src/lib.rs:7-8 は「gpu-paint / paint-engine の内部にも計測点を挿せるよう」と説明しているが、依存しているのは apps/desktop だけ (Cargo.toml の grep で確認) / crates/frame-profiler/src/engine.rs:90 コンストラクタで env::var_os("ALTPAINT_PROFILE") を読む副作用があり、テスト時の挙動が環境に依存する |

再利用できる部品:
- project_sqlite.rs のチャンクコーデック (chunk_bitmap / extract_chunk_pixels / solid_rgba / encode・decode_rgba / apply_chunks / fill_chunk / blit_chunk: 644-724, 954-1034)。境界検査を足して content-addressed store のブロック層に流用する
- project_file.rs のラウンドトリップ / 版拒否 / 合成ゴールデンのテスト群 (165-548)。新スキーマの回帰仕様として使う
- ProjectStoreError / PenExchangeError の細粒度エラー設計 (thiserror)
- pen-io のバイナリパーサ: ABR v1/v2/v6 sampled + PackBits (abr.rs)、GBR の入出力 (gbr.rs)、境界検査付き読取ユーティリティ (bytes.rs)
- PenImportReport / PenImportIssue の import 診断モデルと、AltPaintPen↔PenPreset の明示 DTO 変換パターン (pen_format.rs)
- json_store.rs の Loaded / Missing / Corrupt 3 状態ローダ (アトミック save と退避を足して完成させる)
- platform/paths.rs の純関数 (resolve_user_data_dir / resolve_asset_dir) とそのテスト、platform/dialogs.rs の DesktopDialogs trait と TestDialogs による差し替え
- frame-profiler クレート全体
- BackgroundJob / JobKind の骨格。単一ライタキューと完了時の状態確定プロトコルの土台にする

### 変更履歴・文書・プロセス (git 履歴 / docs/*.md / docs/adr 001〜019 / docs/tmp / docs/memo.md / CLAUDE.md / .context の実行痕跡)

- 判定: **refactor-in-place**
- 判定の理由: 履歴と文書から見る限り、崩れた原因は主にプロセス側にある。要件の正本の不在、構造指標だけの完了条件、レビューできない速度の自動エージェント実行、未統合の長寿命ブランチ、変更履歴として積み重なる文書である。いずれもコードを作り直しても直らない。作り直しは「最大の構造作業」であり、同じパターンを増幅する可能性が高い。一方で、依存方向の骨格、テスト 572 件、用語体系、GPU 描画の基盤は、往復の末に収束しつつある。したがってこの領域の方針は、コードの破棄ではなく、文書とプロセスをリセットしたうえでの段階的修正とする。具体的には (1) 構造リファクタを止める期間の宣言、(2) SKETCH の要件部分を生きた MVP チェックリストに戻す、(3) 現ブランチを E2E スモークで検証して main に統合しタグを打つ、(4) 文書を現況 3 本に絞り、履歴はアーカイブへ移す、(5) ADR の索引と Superseded の運用、(6) エージェント作業の単位・差分上限・人間の承認ゲートを規約化する。そのうえで、GPU/CPU 二重正本の不具合（テキスト・書き出し・スナップショット）を最初の機能修正として片付けるのが妥当である。
- 要約: altpaint の設計が崩れた主因はコードの書き方より開発プロセスにある。コードベースを捨てるかどうか以前に、プロセスを直す必要がある。385 コミット中、ユーザーが使える機能を増やしたコミットは手分類で約 27 件（約 7%）しかない。そのうち 24 件は最初の 8 日間（03-09〜03-16）に集中している。2026-04-24 以降の 290 コミットで新しく使えるようになった機能は、ほぼ「パネル手動リサイズ」1 件だけである。一方、方式の大転換は 3.5 ヶ月で 10 回を超える。パネル UI（Slint → 自前 DSL/PanelTree → DSL→HTML 翻訳 → HTML 直書き）、描画（render クレート中心化 → 削除、GPU 任意 → 必須 → CPU フォールバック復活）、クレート構成（過去に存在したクレートパス 70 のうち 40 が消滅）が、それぞれ往復している。ADR は 19 本あり記録の習慣は良いが、完了済み 18 本のうち 7 本が 1〜49 日で覆されている。しかも状態欄が更新されず、どの決定が今も有効か判別できない。完了条件は「テスト 0 failed・clippy 0・grep で旧名 0 件」という構造指標ばかりである。未達の受け入れ条件（B10 の仮想 feature ウォークスルー、B8 のレイテンシ実測）も「設計選択の帰結」と書き換えて完了扱いにしている。06-12〜14 の 3 日間に 262 コミット（最大 110/日）があり、.context/b*-workflow.js による自動多エージェント実行で、人間が…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| history-docs#0 | critical | process | ユーザー価値の機能開発が 4 月下旬以降ほぼ止まり、変更の大半が構造改変になっている | git log: 6月の 262 コミットは refactor 229 / docs 24 / fix 4 / feat 0 / 2026-04-24 以降の 290 コミットで追加されたユーザー機能は、パネル手動リサイズ（e6f84f6, 05-15）とカラーパレットの Lab/Oklch（f07489e, 04-23）程度 |
| history-docs#1 | critical | requirements-gap | 要件と優先順位の正本がない（SKETCH はアーカイブ化、ROADMAP には今後の項目が 0 件） | docs/SKETCH.md:1-7 が B10（BL-163、refactor 設計書:565）で「アーカイブ（初期構想）」扱いになった / docs/ROADMAP.md:5-28 はすべて完了フェーズの表。:68-153 も完了済みの Phase 9 の記録で、今後やる項目が存在しない |
| history-docs#2 | high | architecture | 方式の大転換が 10 回以上あり、同じサブシステムを往復している | パネル UI: 719959e（03-09）で Slint を脱却 → 自前 .altp-panel DSL + PanelTree + CPU ラスタライズ（2e9e957）→ DSL→HTML 翻訳器（ADR 009, 04-26）→ 翻訳器 619 行を 7 日後に削除して HTML 直書きへ（ADR 012 / e6f84f6）→ PanelTree 撤去（ADR 014）→ SDK・ABI の再設計（B9）→ テーマ CSS（ADR 019, 進行中） / 描画: Phase 5（03-12）で render クレートを中心に据え、5,244 行まで成長（bfc7811 時点）→ A |
| history-docs#3 | high | process | ADR の決定が短期間で覆り、状態が更新されない | ADR 008（04-25）→ ADR 013（05-12）で 17 日後に撤回。ADR 009（04-26）→ ADR 012（05-03）で 7 日後。ADR 016（06-12）の workspace-persistence → app-core 統合は、翌日の B5（BL-075）で panel-workspace へ再移設。ADR 017（06-12）の panel-runtime による panel-api 再公開は、翌日の B6（C9/BL-091）で解体 / docs/adr/008:5 と docs/adr/009:5 は今も「ステータス: Accepted」のまま。Supe |
| history-docs#4 | high | process | 未達の完了条件を「完了」と宣言している（ゴールポストの移動） | refactor 設計書:670 の B10 完了条件は「仮想 feature の触るファイルが自スライス + names の追記 + 自パネルのみで完結すること」。実際には 5 つの水平箇所の編集が必要だった（ADR018:129-149）。これを「設計選択の帰結」と再定義してバッチを完了にした（ADR018:164-168,206） / refactor 設計書:656 の B8 完了条件「ストロークレイテンシの profiler 計測値を ADR に記録」について、実測せず「構造的に非劣化」と判断した（ADR018:114） |
| history-docs#5 | high | correctness | 構造リファクタの陰で、ユーザーに見える不具合が残っている／新たに生じている（GPU 経路と CPU 状態の二重正本） | IMPLEMENTATION_STATUS.md:1214（04-26）に「text_render 経路は CPU bitmap → GPU 同期が未配線」とあり、現在もそのまま。apps/desktop/src/features/text/service.rs:41-58 の render_text_to_active_layer は CPU 側の Document に書くだけで、GPU へのアップロードも再同期もしない / apps/desktop/src/app/background_tasks.rs:39 の保存では sync_gpu_bitmaps_to_cpu を呼ぶが、:56-5 |
| history-docs#6 | high | docs-drift | 文書が「現況」ではなく変更履歴の積み重ねで、コードと食い違っている | docs/IMPLEMENTATION_STATUS.md は 1541 行。先頭 200 行だけで約 28.5k トークンあり、Read ツールの上限を超える。:5 は「2026-06-12 時点」のまま。:224-437 の「実装済みの主要領域」は app-core / render / storage / canvas / plugin-sdk / ui-shell を現況として記述している（但し書きつき） / IMPLEMENTATION_STATUS.md:9,442,842 と ROADMAP.md:51 が、アーカイブ済みの CURRENT_ARCHITECTURE.md を「現 |
| history-docs#7 | high | process | AI エージェント主導の自動バッチ実行による、レビューできない速度と局所最適 | .context/b4-workflow.js〜b10-workflow.js（例: b7p2-workflow.js の meta・RESULT スキーマ・COMMON プロンプト）でバッチを自動の多エージェント実行にしている / ADR018:11,187 に「19 エージェントによる全クレート調査」「judge panel 3 レンズ × 3 ラウンド」とある。調査結果は .context/survey-results-2026-06-12.json |
| history-docs#8 | high | process | 統合とリリースがない（main から 4.5 ヶ月乖離、未コミット作業が約 2.5 ヶ月放置） | ローカル main は bf4062b（2026-04-29）、origin/main は 1c45d57（05-02）。HEAD は origin/main より 265 コミット先行し、origin/refactor/naming-and-boundaries は未マージ / PR は通算 9 本（Merge pull request #8, #9） |
| history-docs#9 | medium | architecture | 「標準パネルは API 境界が固まってからプラグイン化」という方針に反し、初日から全パネルを Wasm 化した | SKETCH.md:231「標準パネルも将来的にプラグイン化できる構造を目指すが、MVPではまず本体モジュールとして実装し、APIの境界が固まってから順次分離する」 / 実際には 03-09 の c1ea458「Refactor built-in plugins into panel SDK structure」の時点ですでに Wasm プラグイン化していた。以後、ABI と SDK の変更のたびに 12 パネルを改修している（B9 は 1 パネル 1 コミットの移行 12 件、docs/adr/018:204） |
| history-docs#10 | medium | abstraction-mismatch | 垂直スライス再編が、ペイントアプリで支配的な変更の種類に合っていない | ADR018:129-149 のウォークスルー: レイヤー不透明度を 1 つ足すのに、DocumentCommand、document.rs::apply、request_translation.rs::translate_layer、panel-protocol の LayerState、panel-runtime の DocumentSection の 5 つの水平箇所を編集する必要がある / 原則が成り立つのは service 系（I/O）の feature だけ（ADR018:166） |
| history-docs#11 | medium | correctness | 後方互換を捨てる方針と、フォーマットバージョンの運用が食い違っている | crates/project-store/src/types.rs:10 の CURRENT_PROJECT_FORMAT_VERSION = 7 は、55e4e8a（03-11）以降一度も上がっていない（git log -G で確認） / その間に、B0 の BL-003（LayerNode / root_layer のカラム削除）、B1 の 090ec3b（SQLite の panels→komas などテーブル・カラム改名、「マイグレーションなし」）、B5 の BL-079（EditorSession をプロジェクトファイルから除外）と、ディスク上の形式が変わっている |
| history-docs#12 | medium | process | フェーズや ID の体系が多重化し、追跡できない | Phase 0-7、7-bugfix、7-testfix、8A-8G、9A-9G、10-15、16-26（= B0-B10）の番号体系に加えて、BL-001〜165 と C/K/R/P/D の改名 ID（refactor 設計書 §2-3）がある / ROADMAP.md:7-28 の完了フェーズ表は Phase 13 で止まっており、14〜26 が載っていない |
| history-docs#13 | low | dead-or-vestigial | 放置されたプロセス資産（自動化スクリプト・設定・一時計画・ロックファイル方針） | scripts/auto-pull-review-merge-my-ideas.sh:15-17 は、プレースホルダの「特定のフォルダパス」と、存在しない docs/memo/ を対象にしている（.ps1 版も同様） / リポジトリに登録された .claude/settings.json が、/root/rust_programs/altpaint/crates/app-core（WSL のパスで、消滅済みのクレート）への許可ルールを持っている |

再利用できる部品:
- docs/SKETCH.md の要件部分（:53-431: 体験・想定ユーザー・コア概念・MVP・非目標・性能目標）→ アーカイブを解除して docs/PRODUCT.md へ
- ADR 018 の用語体系（§1.2 / ADR018:22-40）と Koma の採用理由
- ADR 001〜019 の本文（歴史記録として。状態欄と Superseded-by を追記したうえで）
- ADR 018 の仮想 feature ウォークスルー（ADR018:123-168）: 作品モデル変更の横断箇所チェックリストの原型として
- CLAUDE.md のコーディング方針（OS 固有コード禁止）と TDD の規律
- refactor 設計書 §5.2 の検証方針（cargo tree による禁止依存の否定形チェック、ラウンドトリップテスト、CPU/GPU ゴールデン等価テスト）
- docs/builtin-plugins/PLUGIN_DEVELOPMENT.md（現況と概ね一致し、外部ロードが未実装であることも正直に書いている）
- docs/RENDERING-ENGINE.md の原則部分（:27-49）
- テストスイート 572 件（特に保存→読込ラウンドトリップ、GPU 差分同期、ゴールデン等価）

### 横断: 依存グラフ・テスト・規約・要件カバレッジ

- 判定: **rewrite-module**
- 判定の理由: 全面破棄は不要で、推奨しません。依存が軽く衛生の良い水平リーフ、GPU シェーダ、SQLite 形式、ブラシのインポータは、書き直しても同等以上のものを作るのに数週間かかる資産です。作り直しで得られるものは限られます。深刻な問題は次の 3 点に集中しています。(a) host コアの状態所有 (CPU/GPU 二重正本、索引キーの履歴、DesktopApp を分割しただけの impl、Document が EditorSession を内包)、(b) 機能に見合わない規模のパネル基盤、(c) リファクタ偏重のプロセス。(a) は局所修正では限界があるため、apps/desktop の app/ + features/paint + features/snapshots + 履歴 + Document / Koma の画素モデルを、1 つのモジュール群として設計し直すのが妥当です。(b) は ABI の凍結とビルトインのネイティブ化を検討します。(c) を先に直さない限り、全面作り直しは同じ迷走を繰り返すだけです。進め方としては、まず GPU 実機の E2E テストで critical な不整合を固定してから、host コアを差し替えます。
- 要約: Cargo 上の依存グラフに循環はありません。水平リーフ (geometry / raster / document-model / editor-state / canvas-geometry) は推移依存 10〜33 パッケージと軽く、本番コードの unwrap / panic / cfg(target_os) もほぼゼロです。層の向きとコード衛生は良好と言えます。一方、重量の大半はパネル UI 基盤 (wasmtime + Blitz/stylo + vello) から来ています。desktop の推移依存 382 のうち 357 は panel-runtime 経由です。パネル基盤は 13.9k 行 (31%) あり、paint と domain の合計 10.5k 行を上回ります。それでいて外部パネルのロード機能は存在せず、ABI / SDK の変更コストだけを払い続けています。最大の構造問題は host 側の状態所有です。GPU テクスチャを編集中の正本にしながら、Document の CPU ビットマップを保存・undo の before・レイヤー/コマ構造同期・export・スナップショットの入力に使い続けています。このため本番の GPU 経路では、レイヤー追加で描画が消える、undo で過去のストロークが消える、PNG export が古い内容を出す、といった不整合がコード上ほぼ確実に読み取れます (実機では未再現)。これが検出されないのは、desktop テスト 208 件が GPU を入れない CPU フォールバックを検証していて、GPU テスト 36 件…

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| crosscut#0 | critical | state-ownership | CPU/GPU の二重正本により、GPU 本番経路で描画消失・undo 破壊・stale な export が起きる (コードからの高確度推測) | apps/desktop/src/features/paint/backend/gpu.rs:8-10 コメントに『GPU 経路でも CPU bitmap を変更しない』と明記 / apps/desktop/src/features/paint/backend/tests.rs:249 gpu_apply_does_not_touch_cpu_pixels がこの不変条件をテストで固定している |
| crosscut#1 | high | process | リファクタ偏重・機能停滞のプロセス (設計迷走の根本原因) | git log: 385 コミット中 refactor 232 / feat 66 / fix 7。2026-06-12〜14 の 3 日間で 262 コミット / git diff --shortstat 6031217 HEAD -- '*.rs': +31,827 / −28,593 (元の 41,664 行の約 7 割を書き換え、正味 +3.2k 行・ファイル数 171→232) |
| crosscut#2 | high | requirements-gap | MVP 要件の大半が未着手または部分実装 | crates/document-model/src/document.rs:578-646 DocumentCommand にページの追加・削除・移動が無い。pages.push は normalize_after_load (:550) と project_file のみ / apps/desktop/src/features/text/raster.rs:41-60 Font8x8Renderer (単行 ASCII のビットマップフォント)。縦書き・フォント・吹き出し関連コードは 0 |
| crosscut#3 | high | test | テストが本番の GPU 経路でなく CPU フォールバックを検証し、GPU テストは GPU 不在時に黙ってパスする。CI も無い | apps/desktop/src/app/tests/mod.rs:104-112 test_app_with_dialogs は install_gpu_resources を呼ばない。interaction.rs 27 件、commands.rs 22 件、persistence.rs 14 件がこれを使う / apps/desktop/src/app/tests/gpu_tests.rs:57-59 と crates/gpu-paint/src/tests.rs:49-51 で `let Some(..) = try_init_device().await else { return;  |
| crosscut#4 | high | correctness | Undo 履歴が文書差し替え・構造変更で無効化されず、ペイント以外は undo できない | apps/desktop/src/features/paint/mod.rs:27-29 clear_edit_history の呼び出し元は features/snapshots/service.rs:40 だけ / apps/desktop/src/app/invalidation.rs:184-190 reset_active_interactions は履歴を消さない。features/project/service.rs:94 (ロード) と app/command_effects.rs:77-83 (NewDocumentSized) から呼ばれる |
| crosscut#5 | high | correctness | コマ追加が既存コマの画素を最近傍リサイズで不可逆に劣化させ、コマ単位ビットマップのモデルが要件と衝突する | crates/document-model/src/document.rs:484-503 add_koma が relayout_page_komas (:708-714) を呼ぶ / crates/document-model/src/document.rs:737-753 resize_koma_to_bounds が全レイヤーと全マスクを resize_bitmap_nearest (:755-776) で書き換える |
| crosscut#6 | high | abstraction-mismatch | パネル基盤への過剰投資: 機能より大きく、外部利用ゼロのまま ABI コストを払い続けている | パネル基盤 13,942 行 (31%) に対し、paint/canvas 6,124 行 + domain 4,427 行 / cargo tree: desktop の推移依存 382 のうち panel-runtime 経由が 357 (wasmtime / blitz / stylo / vello) |
| crosscut#7 | medium | boundary | 『垂直スライス』は DesktopApp の分割 impl にとどまり、所有権・能力の境界になっていない | impl DesktopApp が 29 ブロック / 28 ファイル (うち features/ に 12) / apps/desktop/src/app/services/registry.rs:18 ServiceHandler = fn(&mut DesktopApp, &ServiceRequest)。どのハンドラもアプリ全状態へアクセスできる |
| crosscut#8 | medium | dependency | alpha 依存の未固定と Cargo.lock 未コミットによる非再現ビルド、UI エンジンによる GPU スタックの拘束 | .gitignore:3 Cargo.lock (アプリケーションのワークスペースなのに lock を除外) / crates/panel-html/Cargo.toml blitz-dom/html/paint/traits = "0.3.0-alpha" (^ 指定、解決版は alpha.2)。コメントに『wgpu 28 / vello 0.8 互換のため』 |
| crosscut#9 | medium | test | テストの配置が責務と一致せず、内部実装に密結合でリファクタのたびに書き換わる | crates/raster は 1,010 行に対してテスト 1 件 (src/blend.rs:159)。合成のゴールデンテストは crates/document-model/src/document/tests.rs:863-966 に残る / crates/document-model/src/document/tests.rs:425-470 (view_policy のズーム / パン)、:1005-1019 (ToolKind wire) は editor-state の責務だが document-model 側にある (app-core 解体時にテストが移動されていない) |
| crosscut#10 | medium | boundary | 内部コマンドバスが文字列と JSON で型付けされ、ショートカットが二重経路になっている | crates/panel-runtime/src/services.rs:60-63 ServiceRequest { name: String, payload: Map<String, Value> } / apps/desktop/src/event_loop/keyboard.rs:46-83 host 内部のショートカットまで ServiceRequest::new(names::…) を経由 |
| crosscut#11 | medium | duplication | パネル一覧とツールカタログの多重定義、ビルドスクリプトの不整合 (Linux 用スクリプトで 1 パネル欠落) | scripts/build-ui-wasm.sh の PANELS は 11 件で workspace-layout が無い (grep -c 'builtin-panel-' = 11) / scripts/build-ui-wasm.ps1:21-32 は 12 件 |
| crosscut#12 | medium | docs-drift | 文書の肥大と陳腐化: 『現況』文書が 1,541 行の変更履歴になり、撤去済み DSL の正本や未完の予定コメントが残る | docs/IMPLEMENTATION_STATUS.md は 1,541 行 (約 65k トークン) だが :5 で『短く把握するための現況整理』と自称。:77 と :202 で旧名の残置を自認 / docs/panel-ui-definition/ui-dsl.md:5 が撤去済み (ADR 012) の DSL について『正本である』と記述。docs/tmp/ に 5 ファイル |
| crosscut#13 | low | architecture | クレート粒度のミスマッチ: 薄いクレートが多い一方で host bin が肥大し、HTML スタックは 3 クレートにまたがる | canvas-geometry は本番 366 行 / pub 項目 10、frame-profiler 457 行、geometry 464 行。対して desktop は 15,904 行 (全体の 35%) / crates/gpu-paint/src/brush.rs:9 の editor_state::StrokeMode 1 型のためだけに gpu-paint→editor-state のエッジが存在 |
| crosscut#14 | low | dead-or-vestigial | 痕跡フィールドと未使用の公開 API、陳腐化したリポジトリ設定 | apps/desktop/src/features/tools/catalog.rs:90-94 provider_plugin_id / drawing_plugin_id を非空として必須検証しているが、指し示す 'plugins/default-pens-plugin' (features/tools/default_catalog.rs:20) は存在しない。PaintPlugin は R5 で撤去済み / crates/paint-engine/src/context_builder.rs:36-37 で paint context に伝播し、crates/builtin-panel |
| crosscut#15 | low | performance | 観測性と性能検証の仕組みが無い (ログ基盤なし、性能目標の自動検証なし) | eprintln! 44 件 / println! 44 件。log / tracing クレートの依存 0 / apps/desktop/src/app/tests/interaction.rs:345-615 の #[ignore = "manual performance profiling"] 6 件と scripts/profile-*.ps1 のみ |

再利用できる部品:
- crates/geometry (座標の型区別と矩形演算)
- crates/raster (RgbaBitmap、blend 4 種、BitmapEdit。テストは document-model から移動が必要)
- crates/gpu-paint のシェーダ 7 本と LayerTextureStore / 各 Pipeline (キーを安定 ID 化する前提)
- crates/paint-engine の gesture / PaintPlan / 手ぶれ補正 (純データ計画)
- crates/project-store (SQLite + zstd チャンク + format version + round-trip テスト)
- crates/pen-io の .sut / .abr / .gbr インポータ
- crates/panel-workspace (アンカー配置、8 ハンドルのリサイズ、フォーカス巡回)
- crates/panel-protocol の names (wire 名の単一定義点) と translator の診断経路
- apps/desktop/src/presenter/ (wgpu 提示パイプライン) と present_quads/
- apps/desktop/src/app/tests/interaction.rs などのヘッドレス E2E の型 (GPU 経路と公開 API へ向け直して流用)
- editor-state の view_policy / tool_descriptor (純関数)

### 領域横断 (完全性チェック)

- 判定: **rewrite-module**
- 判定の理由: 横断問題の大半は、3 つの決定が欠けていることに帰着する。(1) 誰が状態を所有し、変化をどう伝えるか (revision/ChangeSet)。(2) 作品内で一意な安定 ID。(3) コマ/ページ/画素ストアのデータモデル (タイル化・コマ = クリップ)。この 3 つはドメイン・host コア・保存スキーマ・GPU 常駐に同時に効く。そのため個々のファイルを直すリファクタでは解消せず、モジュール単位の作り直しが必要になる。一方で、依存方向・GPU カーネル・幾何・コーデック・パーサ・ABI 定数・パス解決などは健全で、全面破棄は不要である。順序としては、まずこの 3 点を仕様として文書化し、E2E の受け入れ基準 (日本語テキスト、レイヤー追加後もストロークが残る、閉じる前の確認) を置いてから移行する。その順を守らなければ、作り直しても同じ乖離が再発する。
- 要約: 8 領域の要約は、CPU と GPU の二重正本という共通の症状を正しく捉えている。ただし、その背後にある横断的な根本原因はどの領域でも一つの問題として扱われていない。根本原因とは、ドメインが変化の有無も影響範囲も返さないことである。そのため派生状態を持つ消費者 (GPU テクスチャ、パネル host state、提示、セッション永続化) が、それぞれ独自の変化検出を 6 系統も手書きしている。同様に「レイヤー = 位置 index」と「コマ = コマ寸法の独立ラスタキャンバス」という 2 つの前提が、document-model・gpu-paint・project-store の SQLite スキーマ・desktop の履歴・提示・書き出しに別々に焼き込まれている。作り直しで最も高くつくのはこの部分であり、SKETCH の要件 (コマからの飛び出し、コマの重なり、ページ書き出し) とも衝突する。どの担当にも落ちていない問題も 4 つ確認した。(1) テキスト機能が、入力 (SetText の生成箇所ゼロ、IME はスタブ)・ラスタライザ (font8x8 は ASCII のみで日本語は空白になる)・画素経路 (CPU のみ) の 3 段で、それぞれ独立に不通になっている。(2) Wasm などの実行時アセットの供給経路と、host↔Wasm の版管理が無い。そのためテストまでが、gitignore されたローカルのビルド成果物に依存している。(3) 未保存変更という概念が無く、ウィンドウを閉じると確認もせず即終了する。(4) CPU・GPU・スナップショットが、ページ全面 …

| # | 重大度 | 分類 | 問題 | 主な根拠 |
|---|---|---|---|---|
| critic#0 | high | state-ownership | ドメインが変化を通知せず、派生状態の変化検出が 6 系統独立に手書きされている | crates/document-model/src/document/layer_ops.rs:193-209 add_raster_layer などドメイン操作はすべて () を返し、変化の有無も影響範囲も返さない / apps/desktop/src/app/command_effects.rs:19-80 コマンド種別ごとの副作用を手書きした表 (系統1) |
| critic#1 | high | duplication | 「レイヤー = 位置 index」が 4 サブシステムに独立に焼き込まれ、安定 ID はパネル選択にしか使われていない | crates/document-model/src/document.rs:78,291 LayerNodeId は定義されている / crates/document-model/src/document/layer_ops.rs:198-203 LayerNodeId はコマ内の created_layer_count で採番されており、作品内で一意ではない |
| critic#2 | high | architecture | 「コマ = コマ寸法の独立ラスタキャンバス」の前提が 6 サブシステムに伝播し、SKETCH の飛び出し・重なり・ページ書き出しと衝突している | crates/document-model/src/document.rs:224-241 Koma が layers と composite_cache を所有し、レイヤー寸法 = コマ寸法になっている / crates/document-model/src/document.rs:708-755 relayout_page_komas → resize_bitmap_nearest で、全コマの画素を最近傍法で破壊的にリサイズする |
| critic#3 | high | performance | メモリ予算が存在せず、全層がページ全面 RGBA の完全な複製を独立に保持している | crates/document-model/src/document.rs:12-13 既定ページ 2894×4093 (RGBA 1 枚 = 45.2 MiB) / crates/raster/src/bitmap.rs:13-21 RgbaBitmap は Vec<u8> で、Clone は深いコピー |
| critic#4 | high | requirements-gap | テキスト機能が入力・ラスタライズ・画素経路の 3 段で独立に不通になっている | apps/desktop/src/features/text/raster.rs:97-101 font8x8::BASIC_FONTS (U+0000〜007F) のみ使い、範囲外の文字は空白になる。日本語は何も描かれない / apps/desktop/src/event_loop/keyboard.rs:18-20 handle_ime_event は常に false (IME 未対応)。:22-36 キー入力はショートカット解決にしか使われない |
| critic#5 | high | process | 実行時アセットの供給経路と host↔Wasm の版管理が未設計で、開発ループとテストがローカルの未追跡成果物に依存している | .gitignore:4-5 crates/builtin-panels/**/*.wasm と plugins/**/*.wasm を無視している。.gitignore:3 で Cargo.lock も未追跡 / crates/panel-runtime/src/loader.rs:30-81 同梱 12 パネルの固定リスト。:103-106 ロード失敗は診断文字列を積むだけ |
| critic#6 | high | requirements-gap | 未保存変更の概念が無く、ウィンドウを閉じると確認も保存もせずに即終了する | apps/desktop/src/event_loop.rs:223 WindowEvent::CloseRequested => event_loop.exit() (確認なし、保存なし、保存ジョブの join なし) / リポジトリ全体で unsaved / is_dirty / modified 系の状態は 0 件 |
| critic#7 | medium | abstraction-mismatch | データ駆動を装った固定実装と、単一実装しかない拡張点の過剰 | tools/builtin/pen.altp-tool.json の settings[] は label/min/max を持つが、消費されるのは crates/panel-runtime/src/host_state.rs:391-417 の supports_* 真偽値 4 個と、JSON 文字列の丸渡しだけ / crates/builtin-panels/tool-settings/panel.html:12 のスライダーは 0..1000。一方 JSON・editor-state/src/session.rs:209・tool-settings/src/lib.rs:12 は 1.. |
| critic#8 | medium | requirements-gap | 入力モデルが OS 境界で精度と意味を落としている | apps/desktop/src/event_loop.rs:231,237 PhysicalPosition<f64> を as i32 で切り捨てている。サブピクセル精度が失われ、縮小表示時は 1 ウィンドウ px が複数キャンバス px に対応するため、線が階段状になる / apps/desktop/src/event_loop/pointer.rs:86-108 DeviceEvent::MouseMotion の raw 差分 (加速なしのデバイス単位) を直前のカーソル座標に足して、ストロークに流している。OS カーソルと乖離し、次の CursorMoved で引き戻される可能性が |
| critic#9 | medium | architecture | ユーザーへのエラー・診断の経路が全層で欠けている | 本番コードの eprintln! は 33 箇所 (app/bootstrap.rs 4、features/workspace/service.rs 3、features/tools/service.rs 3、features/snapshots/service.rs:28,35、app/services/gpu_sync.rs:33 ほか) / crates/panel-runtime/src/loader.rs:89-109 register_builtin_panels は Vec<String> の診断を返し、apps/desktop/src/app/bootstrap.rs:104- |
| critic#10 | low | boundary | editor-state が「共有カーネル」化し、GPU・幾何・保存層が一過性のセッション型に依存している | crates/gpu-paint/Cargo.toml → editor-state (StrokeMode のため) / crates/canvas-geometry/Cargo.toml → editor-state (CanvasViewTransform のため) |
| critic#11 | low | duplication | 色の表現が 4 形態あり、hex パーサが同一実装で二重になっている | crates/editor-state/src/session.rs:15-23 ColorRgba8 / crates/panel-sdk/src/commands.rs:4-19 RgbColor (alpha なし) |
| critic#12 | low | docs-drift | 保存ファイルの名前と中身、および追跡状態が食い違っている | apps/desktop/src/platform/paths.rs:15 既定のプロジェクト名は altpaint-project.altp.json だが、中身は SQLite (crates/project-store/src/project_file.rs:19-27 がヘッダを検査。リポジトリ直下の同名ファイルも 'SQLite format 3' ヘッダ) / 旧 JSON 形式のプロジェクトは読込時に InvalidProject で拒否され、移行経路は無い (project_file.rs:19-27) |

再利用できる部品:
- crates/panel-runtime/src/host_state.rs:129-150 の section + revision キャッシュ (ドメイン発行の revision に差し替えて、アプリ全体の変更伝播の雛形にする)
- crates/project-store の layer_chunks のチャンクコーデック (キーを (LayerId, tile) に替えてタイル化画素ストアの保存に流用)
- apps/desktop/src/platform/paths.rs の純関数群 (resolve_user_data_dir / resolve_asset_dir) とそのテスト
- crates/panel-protocol/src/abi.rs と names.rs の単一定義と一意性テスト (版番号の追加、操作語彙の抽出)
- apps/desktop/src/app/services/registry.rs の feature 登録表の形 (型付きコマンドへの移行後も流用)
- geometry の座標 newtype、gpu-paint の WGSL カーネル、canvas-geometry の純関数 (他領域の評価と同じ)

## E. 3 案の要約と審査結果

3 案は独立に作成し、2 観点の審査員がそれぞれ採点した。
本 ADR は骨格再建案を採り、止血案を「切り替え先」とする (本 ADR 4.3 節、10 章 Q1〜Q3)。
切り替えるときは、本節の止血案をそのまま計画として使う。

### E.1 審査の点数 (各 50 点満点)

| 案 | オーナー価値 (実用到達の速さ / 停滞リスク / 資産活用 / 動機との整合 / 見積もりの正直さ) | 再発防止 (3 決定の解消 / プロセス対策 / 転換の抑止 / 本番経路の検出 / SKETCH 整合) | 合計 |
|---|---|---|---|
| 骨格再建 (greenfield) | 29 (5 / 5 / 4 / 9 / 6) | 40.5 (9 / 8 / 6.5 / 9 / 8) | 69.5 |
| その場で差し替え (strangler) | 27 (3 / 4 / 7 / 6 / 7) | 35.5 (7 / 7 / 6 / 8 / 7.5) | 62.5 |
| 止血して機能を出す (stabilize) | 31 (7 / 7 / 9 / 3 / 5) | 31.5 (4.5 / 9 / 6 / 6 / 6) | 62.5 |

両審査が全案共通の見落としとして挙げたもの:

- オーナーのレビュー時間 (週 8〜12 時間) を当然視している。プロジェクトは 11 週間止まっていた。
- 規約の強制が慣習に偏っている (ブランチ保護・CI・hooks による機械的な強制が弱い)。
- 本監査自体が ADR 018 と同じ「多エージェント調査 → 大規模再設計」の産物である。
- オーナー自身の制作シナリオを一次の受け入れ資料にしていない。
- 観測性 (ログ・クラッシュログ・エラー表示) の基盤が計画に無い。
- ペン入力の忠実度 (ホバー・消しゴム側・傾き) を評価していない。

本 ADR はこれらを、試行期間 (Phase 0〜1)、8 章 16 の機械的強制、制作シナリオの聞き取り、tracing とエラートースト、PenSample とスパイク (b) で手当てした。

### E.2 止血案 (切り替え先) の段階計画

前提: 1 作業日 = エージェントの実装とテスト + オーナーのレビュー 1〜2 時間。見積もりの不確実性は ±50%。

| Phase | 内容 | 規模 | 出口 |
|---|---|---|---|
| 0 基準点と検出力の回復 | ブランチ整理と main 統合、Cargo.lock 追跡と blitz 系の `=0.3.0-alpha.2` 固定、wasm ビルドスクリプト修正、R1〜R16 の再現テストを ignore 付きで移植、GPU テストの catch_unwind 撤去、最小 CI、PRODUCT.md / STATUS.md / 規約 | 3〜5 日 | オーナーが規約を承認し、基準タグを打つ |
| 1 データ消失の止血 | 1 PR ずつ TDD で直す。R10 保存の原子化 (一時ファイル + rename、単一ライタ) → R9 読込失敗の区別 → R11 ディレクトリ作成 → R16 未保存判定と終了前 join → R1〜R5 (commit / undo 後に dirty 領域を GPU→CPU へ write-back、後処理を replace_document に集約) → R6 明示のストローク終了 → R8 履歴を (文書世代, KomaId, LayerNodeId) で特定 → R7 再配置とリサンプルの廃止 → R15 コマを bounds 位置に描画 → R13 描けない文字をエラー通知 | 10〜16 日 (本番差分 1,200〜2,000 行) | 描く → レイヤー追加 → コマ作成 → undo/redo → PNG → 保存 → 再起動 → 読込 のスモークで消失 0 |
| 2 パネル入力の止血と基盤判定 | R12a ホストでスライダー値を算出して SetValue、R12d リスト項目に id を付けパネル上のクリックを遮断、R12c select をボタン群へ、R12b ネイティブ 1 行入力ダイアログ、state patch の read-your-writes、Ctrl+Z / Ctrl+S をホストのキーマップへ | 6〜10 日 (timebox 8 日) | Blitz 継続か egui 移行 (15〜25 日) かを判定して ADR に記録 |
| 3 描ける線とページ | 入力座標を f32 のまま通す、筆圧のセグメント内補間、スタンプ上限撤廃、ブラシ dispatch を dirty 範囲に限定、ページの追加・削除・並べ替え・切替、ページ全体表示 | 12〜20 日 | 2〜3 ページのネームと線画でオーナーが線質を合格とする |
| 4 書き出しと止まらない UI | レイヤー画素の Arc COW、JobRunner、ページ単位 PNG と PDF、readback の非同期化、履歴容量のバイト上限化 | 8〜14 日 | 実原稿を PDF にして外部ビューアで確認 |
| 5 コマ割り編集と構造 Undo (核の作り直し) | 作品内一意 ID、ページ座標レイヤーとクリップとしてのコマ、apply が ChangeSet を返す、GPU store・履歴・presenter のキーを LayerId に、スキーマ v8、構造操作の Undo、コマの移動・リサイズ・分割・簡易スナップ | 25〜40 日 (最大の不確実性) | コマ割り → ペン入れ → コマ調整で画素消失 0 |
| 6 テキスト流し込み | テキストレイヤー、Markdown 台本、fontdb + swash、縦書き | 10〜18 日 | 自分の台本を流し込んで PDF にできる |
| 7 スナップショット・テンプレート・プロファイル | content-addressed チャンクと snapshots テーブル、差分保存、コマ割りテンプレート、UI 状態の正本を 1 か所に | 15〜25 日 | MVP 10 項目 (第三者パネルを除く) が完了 |
| 8 (条件付き) プラグインと 100 ページ | egui 移行と宣言的 UI ツリー、疎タイルと GPU 作業集合の LRU | 25〜45 日 | 発火したゲートごとの受け入れテスト |

合計: 止血完了まで暦で 5〜8 週、MVP (第三者パネルと 100 ページを除く) まで暦で 6〜9 か月。

### E.3 その場で差し替え案 (参考)

差し替え順は、受け入れテストの足場 → データ消失の止血 → PixelStore と一意 ID (GPU 正本) → 同梱パネルの egui 化 → AppCommand / ChangeSet / 履歴 v2 → ドメイン v2 → テキスト → 保存 v2 → ブラシ。
核の差し替え (P0〜P4) に 16〜24 週 (内訳: P0 1.5〜2.5 週、P1 1.5〜2.5 週、P2 4〜6 週、P3 5〜7 週、P4 4〜6 週。新機能が出ない P2〜P4 は 13〜19 週)、MVP 機能 (P5〜P8) に 19〜29 週、合計 35〜53 週。
本 ADR が取り込んだ要素は、Driver ファサード、見積もりの 150% で停止する規則、ジョブ結果の epoch 検査である。

## F. 規模の内訳 (推定)

本 ADR 1 章・4.1 節の「作り直し 約 17〜19k 行」「移植 約 15.5〜16k 行」の内訳。
Q6 の回答 (Blitz の継続、全パネルを同じ扱いにする) を反映した (2026-09-23)。
反映の前は、パネル基盤の約 11k 行を作り直しに数え、作り直し 約 23k 行・移植 約 10〜11k 行としていた。
行数はテストを含む Rust の行数 (af4ec88 時点、`git ls-files '*.rs' | xargs wc -l`) からの推定である。

| 区分 | 対象 | 行数 (概数) |
|---|---|---|
| 作り直し | パネルとホストの接続 (panel-runtime の host state・翻訳器・要求の経路・読込と永続設定 約 3.5k、panel-wasm-host の状態の取得と要求の発行 約 0.4k、panel-sdk の文字列の要求経路 約 0.3k、同梱パネルの Rust のうち状態の受け取りとハンドラ 約 1.5〜2k) | 約 5.5〜6.5k |
| 作り直し | desktop の状態・効果モデル (DesktopApp、command_router / effects、invalidation、registry、host_request_router、panel_config_sync、background_tasks) | 約 3〜4k |
| 作り直し | 提示の糊層 (present.rs / frame.rs / present_api.rs / invalidation の提示部分 / cpu_canvas_snapshot) | 約 1.2k |
| 作り直し | ドメイン (document-model の製品コード、editor-state の session) | 約 2.4k |
| 作り直し | ペイント (features/paint の backend と history、paint-engine の PaintEngine と整数スタンプ、raster のラスタライザ) | 約 3k |
| 作り直し | 永続化 (project-store のスキーマと段取り、session / preset の保存) | 約 1.5〜2k |
| 移植 | pen-io | 約 2.4k |
| 移植 | presenter のパイプライン・シェーダ・present_quads | 約 2〜2.5k |
| 移植 | geometry / canvas-geometry / raster::blend | 約 1.3k |
| 移植 | gesture・手ブレ補正・ToolDescriptor・view_policy・koma/gesture | 約 0.8k |
| 移植 | チャンクコーデック、json_store、platform (paths / dialogs)、frame-profiler | 約 1.5k |
| 移植 | event_loop の pointer.rs / keyboard.rs、gpu-paint の readback.rs / pipeline.rs | 約 0.65k |
| 移植 (Phase 2) | パネルの表示と実行 (panel-html 約 1.5k、panel-wasm-host のうち DOM の host function と実行の骨格 約 1k、panel-protocol の ABI 定数・names・keyboard 約 0.5k、panel-sdk と panel-macros のうち ShortcutRegistry と文字列の要求経路を除く部分 約 1.7k) | 約 4.5〜5k |
| 移植 (Phase 5) | panel-workspace | 約 1.7k |
| 移植 (Phase 2〜6) | 同梱パネルの純ロジック (色空間変換、対数スライダー、ラベル整形。パネルごとに、その機能を入れるフェーズで移植する) | 約 0.5k |

作り直しの合計は約 17〜19k 行である。
移植の合計は約 15.5〜16k 行で、Rust 45k 行の約 35〜36% にあたる。
このうち約 11k 行は動作とテストで検証済みの部品で、パネルの表示と実行 (約 4.5〜5k 行) は R12 の入力まわりを直しながら移植する。
panel-sdk の ShortcutRegistry (約 0.3k 行) は持ち込まず、ショートカットはホストのキーマップ (desktop の状態・効果モデルの作り直し) で扱う。
同梱パネルの `panel.html` / `panel.css` (約 0.5k 行) は Rust ではないので、上の行数に数えていない (パネルごとに、その機能を入れるフェーズで移植する)。
残りのテストコード (desktop 約 7k 行など) は移植しないが、シナリオは ACCEPTANCE.md の受け入れ条件として引き継ぐ。

## H. 運用記録

(Phase 0 以降に追記する。Phase 0 より前の記録は H.1 と H.2 に置く。)

### H.1 ADR 020 の簡易レビューの収束時点で残った SUGGESTION (2026-09-23)

ADR 020 は 40 回の簡易レビューを経て ISSUE 0 件で収束した。
次の SUGGESTION は方針の変更を伴わないので本文に入れず、Phase 0 で運用文書と付録 G に落とし込むときに扱う。

1. 止血案へ切り替えた後は、11 章 1 の「付録 H」を止血案の ADR の運用記録の付録と読む、と明記する。
2. 止血案での 8 章 5 (green 0 件の規則) の起点を「撤退の手順 3 に着手した翌週」とする (切り替え作業の最中に誤って発動しないように)。
3. 打ち切りで確定した付録 G に残った Issue は、Phase 0 の試験の前に、細目 (検知漏れの是正) か 8 章 16 の本文の改訂で解消する。
4. 11 章 1 の「改訂の開始」の起点を「最初の簡易レビューにかけた日」とする。
5. 打ち切って確定する規則はグローバル CLAUDE.md の「ISSUE が残る Plan を提示しない」に対するプロジェクト側の例外であり、エージェントは収束しないことをオーナーに報告して判断を仰ぐ (Plan としては提示しない)、と明記する。
6. 付録 G の打ち切り (3 週) の後に設定と試験が残ると Phase 0 は 150% を超えるので、そのときの再計画で決めること (残りの設定と試験の期限) を付録 G に添える。
7. 11 章 1 の再レビューの規則 (打ち切りの期限) 自体も、再レビューの対象に含める。

### H.2 ADR 020 の分割と Q1〜Q7 の回答 (2026-09-23)

- 22:15 JST から、ADR 020 を本体・5 章・6 章と 11 章・8 章の 4 ファイルに分けた。分けただけの版で、分割の前後で空行以外の行の集合が一致することを確かめた。
- 同じ日に Q1〜Q7 の回答を 10 章に記録した。Q6 (推奨以外: Blitz の継続、全パネルを同じ扱いにする) と、Q4 に添えたオーナーの判断 (GPU ストローク描画) による改訂は、11 章 1 の手順で再レビューにかけた。
- 改訂は 3 回の簡易レビュー (第 41〜43 版) で ISSUE 0 件になった。残った SUGGESTION は次の 1 件で、Phase 2 のパネルの PR で決める: Phase 2 のパネル 2 つを preset の位置と大きさに固定で置くとき、preset の 4 隅の anchor をウィンドウの大きさから矩形に直す最小限の計算を入れるか。

### H.3 Phase 0 のブランチ整理の実施記録 (11 章 2)

- 2026-09-24 手順 1: 未コミットだった ADR 019 の作業 (33 ファイル) をコミット `1dd8a98` に記録し、`legacy-adr019-wip` タグを打った。タグの中身が作業ツリーと一致することを確かめた。
- 2026-09-24 手順 2: `af4ec88` から `chore/legacy-freeze` を作り、Cargo.lock・rust-toolchain.toml・README-LEGACY.md の追加と、.gitignore の修正を入れた (`0b2a199`)。
- 2026-10-12 手順 3: クリーンな clone で README-LEGACY.md の手順どおりにビルドした。`cargo test --workspace` は 568 passed / 0 failed / 9 ignored で、B10 の記録と一致した (`fcfa119`)。
- 2026-10-13 手順 4: PR #11 を `main` にマージした (マージコミット `ff64794`。内容は `fcfa119` と一致する)。11 章 2.4 は「マージはオーナーが行う」としているが、オーナーの指示により、エージェントがオーナーのアカウントで `gh pr merge --merge` を実行した。開いたままだった PR #10 (refactor/naming-and-boundaries) は、これで GitHub 上でマージ済みになった。マージコミットに `legacy-final` タグを打ち、`legacy-adr019-wip` と一緒に origin へ push した。
