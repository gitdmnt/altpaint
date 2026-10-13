<!-- ADR 020 の 8 章の規約に沿って書く。1 課題 1 ブランチ 1 PR。 -->

## 何を変えるか


## 受け入れ ID または Issue

<!-- 下の行の末尾に、受け入れ ID (docs/ACCEPTANCE.md の A-R1 など) か Issue の番号 (#12 など) を書く。
     文書だけの PR と .github だけの PR は、この行を消してよい。 -->
Refs: 

## 確認

- [ ] 実装の前に、テストが失敗したログを残した (コードを変える PR の場合)
- [ ] ラベルを 1 つ付けた (feat / fix / port / structure / docs / ci / test / spike)
- [ ] 手書きのソースの追加は 400 行以内、変更ファイルは 15 以内

<!-- 移植 PR (上限 800 行) の場合だけ、次の形の行を行頭から書き、元のコミット・変えた前提・持ち込んだテストを添える。
       Ported-from: legacy-final:crates/pen-io/src/abr.rs
-->
