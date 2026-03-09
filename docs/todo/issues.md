# known bugs

- [x] AppパネルのKeyboardを押すとフリーズする
- [x] 最初に開くファイルが固定
- [x] Canvas hostの左右にキャンバスが表示されない領域がある。もう少し調査したところ、ドキュメント作成に伴うキャンバス初期化時に初期化された領域外に、キャンバスを移動しても描画が更新されない
- [x] Layerのindex, blend, visible, maskにプレイスホルダーしか表示されない
- [x] ToolsのPresetとSize, Penのpen_sizeにもプレイスホルダーしか表示されない
- [x] Penのサイズスライダーが機能しない (unsupported command payload type in panel-sdk runtime)
- [x] プラグインの表示順序や表示非表示が永続化されない
- [x] saveの実行時にUIスレッドがブロックされる
- [ ] 