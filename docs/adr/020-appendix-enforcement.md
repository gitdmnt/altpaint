# ADR 020 付録 G: 強制の設計

- 作業日時: 2026-10-13 JST
- 作業 Agent: Claude Opus 5.5 (claude-opus-5-5、Claude Code)
- 位置づけ: ADR 020 の 8 章 16「機械的な強制」が、この付録に委ねた設計を決める。8 章 16 の本文 (想定する相手、守る結果、構成の原則、逸脱時の対応、試験の原則) は変えない。
- 前提: 10 章 Q7 の回答は「エージェント専用のアカウントを用意しない」である。エージェントもオーナーも、オーナーのアカウント (admin) で動く。
- この付録は、追加した後は初期設計の記録として更新しない (8 章 16)。追加した後の正本は G.12 の運用文書である。

章の中の「守る結果 1〜6」は、8 章 16 の番号を指す。

## G.1 要約

1. GitHub の側は、ruleset を 2 本入れる。main (と Phase 1 の `spike-base`) は「PR 必須、メタ検査 CI 必須、削除と強制 push の禁止、バイパスなし」、保護するタグは「削除と更新の禁止、バイパスなし」とする。承認の必須数は 0 にする。PR の作成者は自分の PR を承認できず、アカウントが 1 つしか無いからである。
2. 承認に当たるのは、オーナーが、レビューした head を指定して行うマージである。オーナーは専用のスクリプトでマージし、スクリプトが控えを残す。
3. GitHub が止めない操作 (合格した PR のマージ、免除ラベル、設定の変更、資格情報、hooks 自身) は、Claude Code の hooks が止める。hooks は、altpaint を対象とするツール呼び出しを判定し、拒否するか、記録して通す。
4. hooks は、ユーザー設定の PreToolUse に 1 本だけ登録する。起動に失敗したら、ツール呼び出しを止める側に倒す。
5. hooks が見えない経路 (スクリプトのファイルの中の操作、hooks の無い環境) は防げない。週次の監査で、GitHub の記録と、hooks のログとオーナーの控えを突き合わせて気づく。
6. 経路の網羅は目指さない (8 章 16 の「想定する相手」)。代表的な書き方を拒否し、漏れは見つけ次第、検知漏れの是正 (8 章冒頭の細目) として塞ぐ。

## G.2 前提として確かめたこと

### この機器とリポジトリ (2026-10-13 の実測)

| 項目 | 値 |
| --- | --- |
| GitHub の保護 | classic のブランチ保護は無い。ruleset は 0 本 |
| リポジトリ設定 | `allow_auto_merge` は false。Actions の既定の権限は read で、Actions による PR の承認は不可。コラボレーターは gitdmnt (admin) だけ。デプロイキー、Webhook、シークレット、変数は 0 件 |
| 必須チェックにするチェック | 名前は `meta-check`、GitHub Actions の `integration_id` は 15368 |
| タグ | 保護の対象は 18 本 (`legacy-*` が 4、`archive/` の下が 14)。どれも注釈付きタグ |
| `gh` | アカウントは gitdmnt の 1 つ。トークンは keyring にあり、スコープは gist、read:org、repo、workflow |
| git | `credential.helper` は `manager-core`。origin は https。`GH_TOKEN` などの環境変数は未設定 |
| Claude Code | デスクトップアプリに同梱の版は 2.1.295。PATH 上の `claude` は 2.1.74 |
| hooks | ユーザー設定にも altpaint の `.claude/` にも、`hooks` と `disableAllHooks` は無い |

### Claude Code の hooks の仕様 (2026-10-13 に公式文書の原文で確かめた)

- PreToolUse は、MCP のツールを含むすべてのツール呼び出しの前に動く。マッチャー `*` はすべてのツールに当たる。
- 入力は標準入力の JSON で、`session_id`、`transcript_path`、`cwd`、`permission_mode`、`tool_name`、`tool_input`、`tool_use_id` を含む。`cwd` は作業中のディレクトリに追従する。
- 終了コード 2 でツール呼び出しを止め、標準エラーの内容が理由としてモデルに渡る。何も出力しない終了コード 0 は「判断なし」で、通常の権限の流れに進む。
- `command` と `args` を並べて書くと、シェルを介さずに実行ファイルを直接起動する。
- hooks が失敗したとき (起動できない、終了コードが 0 でも 2 でもない、タイムアウト、出力が不正) は、既定ではツール呼び出しが進む。`"onFailure": "block"` を付けると止まる。この項目は 2.1.295 以降のものである。
- 設定ファイルの PreToolUse は、サブエージェントの中のツール呼び出しでも動く。
- 設定ファイルの変更は、実行中のセッションにも反映される。
- SessionStart の標準出力は、セッションの文脈に足される。
- `disableAllHooks` は、設定の優先順位 (管理設定、project の local、project、ユーザーの順に強い) で決まった値が効く。altpaint の `.claude/settings.json` に `true` を書くと、ユーザー設定の hooks も止まる。
- フォルダを信頼する前の対話セッションでは、ユーザー設定のものを含めて hooks が保留される。
- クラウドのセッションと GitHub Actions は、ユーザー設定を読まない。

### 文書で確かめられず、試験で確かめること

| 未確認のこと | 確かめる試験 | 期待と違った場合 |
| --- | --- | --- |
| デスクトップアプリのセッションで、ユーザー設定の hooks が動く | T1 の 1 | hooks が動くまで、付録 G の設定は終わっていないものとして扱う (8 章 16 は効き始めない) |
| サブエージェントと Workflow のエージェントの中で hooks が動く | T1 の 22、23 | hooks でサブエージェントを起動するツールを拒否する (8 章 16 の代替) |
| アプリが提供する MCP のツールで PreToolUse が動く | T1 の 4、20 | そのツールを、権限の deny の規則で止める。止められなければ、限界として STATUS に記録する |
| bypassPermissions のモードでも、hooks の拒否が効く | T1 を、普段使うモードのまま行う | altpaint のセッションでは、そのモードを使わない。CLAUDE.md に書き、STATUS に記録する |
| 日本語の拒否の理由が、文字化けせずにモデルに届く | T1 の報告で、理由を復唱させる | 理由の文面を ASCII にする |

## G.3 全体の構成

| 守る結果 | GitHub が止めるもの | hooks が止めるもの | 監査で気づくもの |
| --- | --- | --- | --- |
| 1. main と `spike-base` に、承認した内容以外が入る | 直接 push、強制 push、削除、CI が不合格の PR のマージ | PR のマージ、自動マージの設定、保護したブランチへの push、API でのマージと ref の書き換え | 控えの無いマージ、控えと head が違うマージ |
| 2. 免除ラベルがオーナー以外によって付く | (止めない) | 免除ラベルの付け外し、ラベルの定義の変更 | 控えの無い付け外し、ラベルの一覧の変化 |
| 3. 保護と設定が変わる | (admin のトークンに対しては止めない) | `gh api` の書き込み、設定を変える `gh` のサブコマンド | 設定と期待値の差分 |
| 4. 保護したタグの削除と強制更新 | 削除、更新、強制 push | タグの削除と強制の push、ローカルでの削除と付け替え | タグの指す先の変化 |
| 5. hooks とログの無効化と編集 | (対象外) | hooks の置き場所と設定ファイルへの書き込み | ハッシュの変化、`disableAllHooks`、ログの巻き戻り |
| 6. 資格情報の切り替えと取り出し | (対象外) | `gh auth` の書き込みと取り出し、資格情報ヘルパー、資格情報のファイルの読み取り、環境変数での切り替え | アカウント、スコープ、ヘルパーの変化 |

置き場所は次のとおりとする。

| もの | 置き場所 | エージェントが変えられるか |
| --- | --- | --- |
| hooks の本体、表試験、オーナー用のスクリプト、ruleset の本文、ログ、期待値 | `%USERPROFILE%\.altpaint-guard` (リポジトリの外)。git のリポジトリにして履歴を残し、これをオーナーの dotfiles として扱う | 変えられない (hooks が拒否する) |
| hooks の登録 | `~/.claude/settings.json` | 変えられない (同上) |
| CI の検査 | リポジトリの `.github/` | PR でだけ変えられる。オーナーがレビューしてマージする |
| ハッシュと期待値の写し、入れ直す手順 | docs/STATUS.md | 同上 |

最初の版は、エージェントが `.context/` に下書きし、オーナーが導入のスクリプトで置き場所へ写す。
写した後は、置き場所にあるものが原本である。
置き場所が消えた場合に備えて、オーナーは、置き場所の git のリポジトリの控えを別の場所 (自分だけが読めるリモートか、別のドライブ) に 1 つ持つ。控えには hooks の本体、スクリプト、期待値が入る (ログは入らない)。別の機器への入れ直しも、この控えから行う。

## G.4 GitHub の設定値

classic のブランチ保護は使わず、ruleset にまとめる。
タグの保護は ruleset でしか書けず、ruleset は変更の履歴が残り、管理者にも同じ規則が効くからである。

| ruleset | 対象 | 規則 | バイパス |
| --- | --- | --- | --- |
| `altpaint-main` (`target: branch`、`enforcement: active`) | `~DEFAULT_BRANCH`、`refs/heads/main`。Phase 1 の間は `refs/heads/spike-base` を足す | `deletion`、`non_fast_forward`、`pull_request` (承認の必須数 0、ほかの 4 項目は false)、`required_status_checks` (`meta-check`、`integration_id` 15368、`strict_required_status_checks_policy` は false) | `bypass_actors` は空 |
| `altpaint-tags` (`target: tag`、`enforcement: active`) | `refs/tags/legacy-*`、`refs/tags/archive/*`、`refs/tags/archive/**/*`、`refs/tags/spike-archive/*`、`refs/tags/spike-archive/**/*`、`refs/tags/rebuild-partial` | `deletion`、`update`、`non_fast_forward` | `bypass_actors` は空 |

- `~DEFAULT_BRANCH` と `refs/heads/main` を並べるのは、既定ブランチを変えられても main が外れないようにするためである。
- ruleset のパターンでは `*` が `/` に一致しないので、`archive/*` と `archive/**/*` を並べる。
- タグの ruleset に `creation` は置かない。置くと、これから作る `archive/*` のタグを誰も作れなくなる。
- 作った後に、`current_user_can_bypass` が `never` であることを読み取って確かめる。
- ruleset と一緒に、試験用のラベル `zz-test-exempt` を作る (G.10)。

リポジトリ設定は、今の値がそのまま目標の値なので、変えない。監査の期待値にする。

| 設定 | 期待値 |
| --- | --- |
| `allow_auto_merge` | false |
| Actions の `default_workflow_permissions`、`can_approve_pull_request_reviews` | `read`、false |
| classic のブランチ保護 | 無い (`Branch not protected`、GraphQL の規則数 0) |
| `default_branch`、`visibility`、`archived` | `main`、`public`、false |
| コラボレーター、デプロイキー、Webhook、シークレット、変数 | gitdmnt だけ、0、0、0、0 |
| ラベルの名前の一覧 | 設定した時点の一覧 (改名で免除ラベルを作る経路に気づくため) |
| 保護するタグの ref と、タグのオブジェクトの SHA | 設定した時点の値 (追加は許す) |

`spike-base` は次の順で扱う。

1. Phase 1 の最初に、エージェントが `git push origin origin/main:refs/heads/spike-base` で作る (6 章 Phase 1)。hooks は、`spike-base` への push のうち、この形だけを通す。main の内容はオーナーが承認した内容なので、守る結果 1 に当たらない。
2. オーナーが `altpaint-main` の対象に `refs/heads/spike-base` を足し、期待値を取り直す。
3. オーナーが `rules/branches/spike-base` を読み取り、main と同じ種類の規則が返ることを確かめる。エージェントは `git push --dry-run origin HEAD:spike-base` が hooks に拒否されることを確かめる。
4. 書き込みを伴う試験は、本物の `spike-base` では行わない。ダミーのブランチで、同じ規則の本文を使って行う (G.10)。8 章 16 が本物への書き込みを認める例外に、`spike-base` は入っていないからである。
5. Phase 1 の終わりに、オーナーが対象から外し、ブランチを削除し、期待値を取り直す。

## G.5 hooks

### 登録

`~/.claude/settings.json` に、次の 2 つを足す。`<G>` は置き場所の絶対パスである。

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "*", "hooks": [
        { "type": "command", "command": "<G>\\node.exe", "args": ["<G>\\guard.mjs"], "timeout": 15, "onFailure": "block" }
      ] }
    ],
    "SessionStart": [
      { "hooks": [ { "type": "command", "command": "<G>\\node.exe", "args": ["<G>\\guard.mjs"], "timeout": 15 } ] }
    ]
  },
  "permissions": { "deny": ["Edit(~/.altpaint-guard/**)", "Edit(~/.claude/settings.json)"] }
}
```

- マッチャーを `*` にし、ツール名での分岐は判定の中で行う。ツール名を並べる形は、ツールが増えたときに漏れる。
- シェルを介さない形で起動する。引用符や PATH の影響を受けない。
- Node は、置き場所に写した専用の `node.exe` を使う。この機器の Node は版を切り替える道具の下にあり、切り替えで hooks が動かなくなるのを避ける。
- 権限の deny の 2 行は、hooks の判定とは別の仕組みで同じ場所を守る補助である。deny の規則はどの権限モードでも効く。ログには残らない。

### 止まる側への倒し方

- `onFailure: "block"` で、hooks が起動できないとき、タイムアウトしたとき、出力が不正なときに、ツール呼び出しを止める。
- 判定のスクリプトは 5 秒の期限を自分で持ち、過ぎたら拒否で終わる。例外はすべて拒否にする。
- 対象と判定した呼び出しをログに書けなかったときは、拒否する。許可した呼び出しをすべて記録するという 8 章 16 の定めを守るためである。
- 対象外の呼び出し (altpaint に関係しないプロジェクトの作業) は、入力の JSON を読んだ直後に、ログに書かずに通す。altpaint のための hooks が、ほかのプロジェクトの作業を止める場面を減らす。
- hooks が壊れると、この機器の Claude Code のツール呼び出しがすべて止まる。直せるのはオーナーだけである。復旧の手順は、置き場所の README.md に書く。
- SessionStart の hooks は、altpaint のセッションの最初に `altpaint-guard <ハッシュの先頭 12 桁> が有効` という 1 行を文脈に出す。CLAUDE.md に「この行が無いセッションでは、altpaint への push、PR の作成と更新、ラベルの操作をしない」と書く。hooks が保留されているセッション (信頼する前のフォルダ) や、hooks の無い環境に、エージェント自身が気づくための合図である。

### 対象の判定

次のどれかに当たる呼び出しを「altpaint を対象とする呼び出し」とする。迷う場合は対象に倒す。

| 手掛かり | 判定 |
| --- | --- |
| 作業中のディレクトリ | 入力の `cwd` から上へたどって `.git` を見つけ、origin の URL が `github.com` の `gitdmnt/altpaint` である。worktree にも対応する |
| セッション | 入力の `transcript_path` に `altpaint` がある (altpaint で始めたセッションは、ほかのディレクトリへ移っても対象のまま) |
| 文字列 | ツールの入力に `altpaint`、リポジトリの id (1176119351)、node ID がある。`-R gitdmnt/altpaint`、`git -C <パス>`、PR の URL などを拾う |
| ファイル | ファイルのツールの対象が、altpaint の作業ツリーの中にある |

守る結果 5 の規則 (hooks の置き場所、ユーザー設定、管理設定への書き込み) だけは、対象の判定に関係なく、この機器のすべてのセッションに当てる。
ほかのプロジェクトのセッションが hooks を外しても、altpaint に効くからである。
守る結果 6 の規則は、altpaint を対象とする呼び出しにだけ当てる。ほかのプロジェクトでの `gh auth token` には正当な使い道があり、取り出しの結果は機器に残らないからである。

### コマンドの読み方

`tool_input` に `command` という文字列があれば、ツール名に関係なく読む (Bash、PowerShell、ターミナルのツール、Monitor など)。

1. 引用符を読みながら、命令の区切り (改行、`;`、`&&`、`||`、`|`、`&`、括弧) で節に分ける。Bash と PowerShell で、引用符とエスケープの規則を使い分ける。
2. 単一引用符の中、引用符つきの区切り語のヒアドキュメントの本文、PowerShell の `@' '@` の中は、データとして扱う。コミットメッセージや PR の本文に `gh pr merge` と書いても、拒否しない。
3. 二重引用符の中と、引用符なしのヒアドキュメントの本文では、`$( )` と (Bash の) バッククォートの中だけを命令として読む。実際に実行されるからである。
4. 節の先頭から、予約語 (`do`、`then` など)、変数への代入 (`NAME=値`、PowerShell の `$x =`)、ラッパー (`env`、`timeout`、`xargs` など) を取り除き、実行ファイルの名前を取り出す。ディレクトリ、`.exe`、大文字と小文字の違いは無視する。
5. 規則は、節の先頭が `git` か `gh` のとき、その引数の並びに当てる。文字列の全体には当てない。
6. 入れ子のシェル (`bash -c`、`powershell -Command`、`cmd /c`、`eval`、`Invoke-Expression`、`Start-Process`、`find -exec`、ヒアドキュメントやパイプで文字列を渡したシェル) は、中身をもう一度同じ手順で読む。
7. 読めないもの (閉じていない引用符、`-EncodedCommand`、変数だけの入れ子のシェル) は、対象の呼び出しと、`.claude` に触れる呼び出しでは拒否する。
8. 同じコマンドの中の前の節が、後ろの節の意味を変える場合は、それを引き継ぐ。`cd` と `git checkout` の後は、現在のブランチが読めないものとして扱う (`git checkout -b <名前>` のように名前が読める場合は、その名前を使う)。守る場所へ `cd` した後の書き込み、守る場所の一覧をパイプで削除のコマンドに渡す形、守る場所のパスを入れた変数も読む。

### 規則

「禁止」は、オーナーが行う操作である。「書き方」は、操作は禁止でないが、この書き方では判定できないものである。

| 守る結果 | 分類 | 拒否するもの | 種類 |
| --- | --- | --- | --- |
| 1 | `merge.gh` | `gh pr merge` (`-R` の位置、`--auto`、`--admin`、`--help` を問わない) | 禁止 |
| 1 | `merge.app` | PR の自動マージを設定するアプリのツール | 禁止 |
| 1、2、3、4 | `api.write` | `gh api` の GET 以外。GraphQL は、`query` が引数に読めて `mutation` を含まないものだけを通す | 禁止 |
| 1、2、3、4 | `api.write`、`api.opaque` | メソッドの指定が無く `-f`、`-F` がある `gh api` (`gh` は POST で送る。読み取りなら `--method GET` を付ける、と案内する)。`query` に変数やヒアドキュメントを使った GraphQL | 書き方 |
| 1 | `push.protected` | 宛先が `main`、`spike-base`、`zz-test-base` の `git push` (`HEAD:main`、`refs/heads/main`、現在のブランチが main のときの引数なしの push を含む)。G.4 の `spike-base` の作成の形だけは通す | 禁止 |
| 1、4 | `push.force` | `+` で始まる refspec | 禁止 |
| 1、4 | `push.noncanonical`、`push.opaque` | 許可する形に当たらない `git push`。origin 以外のリモートや URL、`--force`、`--mirror`、`--all`、`--tags`、`refs/heads/` を付けない削除、変数を含む ref 名、`cd`、`-C`、`git checkout <名前>` の後で宛先が読めない push | 書き方 |
| 1 | `push.remote` | origin の付け替え、改名、削除 (`git remote` と、`git config remote.origin.url`) | 禁止 |
| 2 | `label.exempt` | `gh pr` と `gh issue` の `edit`、`create` で、ラベルの旗の値に `size-exempt`、`files-exempt` があるもの (付ける、外すの両方) | 禁止 |
| 2 | `label.opaque` | ラベルの旗の値に変数があるもの | 書き方 |
| 2 | `label.def` | `gh label` の `list` 以外 (作成、改名、削除、複製) | 禁止 |
| 3 | `settings.gh` | `gh repo` の `view`、`clone`、`list`、`set-default` 以外。`gh secret` と `gh variable` の `list`、`get` 以外。`gh workflow enable`、`disable`。`gh ruleset` の読み取り以外 | 禁止 |
| 1、3 | `net.github-write` | `curl`、`Invoke-RestMethod` などで、github.com に、GET 以外のメソッド、本文、認証を付けて送る要求。認証の無い GET は通す | 禁止 |
| 4 | `tag.delete`、`tag.force`、`tag.reserved` | タグの削除の push、タグへの `--force-with-lease`、`legacy-*` と `rebuild-partial` の新しいタグの push と、その名前での `gh release create` (この 2 種類はオーナーが打つ) | 禁止 |
| 4 | `tag.local-rewrite` | 保護する名前のタグの、ローカルでの削除と付け替え (`git tag -d`、`-f`、`git update-ref`) | 禁止 |
| 4 | `tag.opaque` | 名前を変数や一覧の出力で渡した、タグの削除と付け替え | 書き方 |
| 4 | `tag.gh` | `gh release delete`、`edit`、`delete-asset` | 禁止 |
| 5 | `guard.path` | Edit、Write での、置き場所、ユーザー設定、管理設定、altpaint の `.claude/settings.json` と `.claude/settings.local.json` への書き込み | 禁止 |
| 5 | `guard.cmd` | 同じ場所を、引数、リダイレクトの宛先、複写の宛先、パイプで渡す先に取る、読み取り以外のコマンド。`~/.claude` そのものの移動、削除、展開。守る場所へ `cd` した後の書き込み。管理設定のレジストリの操作。読み取り (`cat`、`Get-Content`、`jq`、複写の元にする、など) は通す。`~/.claude` の下でも、設定ファイル以外 (スキルやコマンドの定義など) への書き込みは止めない | 禁止 |
| 6 | `cred.gh-auth` | `gh auth` の `status` 以外 (`token`、`switch`、`login`、`logout`、`refresh`、`setup-git`)、`status --show-token`、`gh config get oauth_token` | 禁止 |
| 6 | `cred.extract` | `git credential` と資格情報ヘルパーの直接の呼び出し、`cmdkey`、`vaultcmd`、`$GH_TOKEN` などの参照 (`printenv`、`Env:`、`env \| grep` を含む) | 禁止 |
| 6 | `cred.switch` | `GH_TOKEN`、`GITHUB_TOKEN`、`GH_HOST`、`GH_CONFIG_DIR` などの代入、`git config` と `git -c` での `credential.*`、`url.*.insteadOf`、`http.*.extraheader` の変更、資格情報を埋めた URL のリモート、`gh ssh-key` と `gh gpg-key` の追加 | 禁止 |
| 6 | `cred.file` | `gh` の `hosts.yml`、`.git-credentials`、`~/.ssh/id_*`、`.netrc`、Claude Code の資格情報のファイルの読み書き (Read、Grep と、コマンドの引数) | 禁止 |
| 道具 | `tool.browser` | アプリ内のブラウザ、Chrome の拡張、デスクトップ操作のツール | 禁止 |
| 道具 | `tool.delegate` | クラウドへの移動、ほかのセッションへの送信と作業の切り出し、リモートのトリガーの作成と実行、リモートで分離したサブエージェント、入れ子の `claude` とほかのエージェントの CLI (`--version` と `--help` だけの呼び出しは通す)、`gh copilot`、`gh agent-task`、`gh codespace`、`@copilot` の割り当て | 禁止 |
| 道具 | `tool.schedule` | スケジュール実行のツールと、OS のスケジューラ (`schtasks` など。一覧の表示は通す) | 禁止 |
| 道具 | `tool.github-mcp` | 名前に `github` を含む MCP のツール | 禁止 |
| 道具 | `gh.indirect`、`gh.unknown`、`git.alias` | `gh alias`、`gh extension`、`gh config set`、一覧に無い `gh` のサブコマンド (別名や拡張の実行)、git の別名の定義 | 禁止、書き方 |
| 道具 | `tool.subagent` | サブエージェントを起動するツール (Agent、Workflow)。T1 でサブエージェントの中の拒否を確かめられなかった場合に、効かなかったツールだけを入れる | 禁止 |
| 道具 | `opaque` | 読めないコマンド (上の 7) | 書き方 |

許可して記録するものは、次の形である。

| 分類 | 形 |
| --- | --- |
| `push.branch`、`push.branch-force`、`push.branch-delete` | `git push [-u] [--force-with-lease] origin <ブランチ>`、`origin HEAD`、引数なしの push、`origin "$(git branch --show-current)"` (現在のブランチを `.git/HEAD` から読む)、`origin --delete refs/heads/<ブランチ>` |
| `push.spike-base-create` | `git push origin origin/main:refs/heads/spike-base` (G.4 の 1。`refs/heads/` を省いた形も同じ) |
| `tag.create` | `git push origin refs/tags/<新しいタグ>`、`git push origin tag <タグ>` (`legacy-*` と `rebuild-partial` を除く) |
| `pr.write`、`pr.read`、`label.normal` | `gh pr` と `gh issue` の作成、更新、コメント、表示。免除ラベル以外のラベル |
| `api.read` | `gh api` の GET |
| `actions.run`、`release.write` | `gh workflow run`、`gh release create` |
| `other` | 上のどれにも当たらない呼び出し (ファイルのツール、ビルド、テスト、サブエージェントの起動など) |

- `legacy-`、`archive/`、`spike-archive/`、`zz-test/` で始まる名前と `rebuild-partial` は、`refs/heads/` を付けずに書くとタグとして扱う。ブランチ名を `legacy-`、`archive/`、`spike-archive/`、`zz-test` で始めない、と CLAUDE.md に書く。
- 強制 push は `--force-with-lease` だけを通す。rebase の後の push に要るからである。レビューの後の追加の push は、hooks では正当な push と区別できない。予防策は、オーナーが head を指定してマージすることである (8 章 16)。
- 免除ラベルを外す操作も拒否する。付け外しをオーナーだけにすると、監査の突き合わせが単純になる。
- サブエージェントを起動するツール (Agent、Workflow) は、許可して記録する。T1 でサブエージェントの中の拒否を確かめられなかった場合は、効かなかった起動のツールの名前を判定の設定 (`DENY_SUBAGENT_TOOLS`) に入れて、拒否に変える (8 章 16 の実行環境)。Workflow でだけ効かなければ、Workflow だけを拒否する。セッションで使えずに試せなかった起動のツールも、効くことを確かめられていないので、拒否に入れる。A-PH0-27 は、Agent の中での拒否を確かめられれば満たす。どれかのツールを拒否に入れた場合は、それが A-PH0-28 の代替であり、A-PH0-29 のとおり CLAUDE.md に書く。Agent まで拒否すると、Plan のレビューをオーナーが別のセッションで行うことになるからである。

### 拒否の文面

拒否の理由は、先頭に `altpaint-guard: deny [分類]` を置き、種類ごとに次の文を続ける。

| 種類 | 文面の趣旨 |
| --- | --- |
| 禁止 | 規約で拒否した。オーナーが行う操作である。別の書き方、スクリプトのファイル、ほかの道具で同じ結果を得ようとせず、オーナーに依頼する |
| 書き方 | 操作は禁止ではないが、この書き方は判定できない。許可する形で書き直す。スクリプトのファイルに移して実行しない |
| 不具合 | hooks の不具合である。オーナーに知らせる。別の方法を試さない |

文面を 2 つに分けるのは、書き方だけの問題までオーナーへの依頼に回ると、エージェントが文面を読み流すようになるからである。

### hooks では防げない経路

| 経路 | 防げない理由 | 気づく方法 |
| --- | --- | --- |
| スクリプトのファイルに書いて実行する操作 (`node x.mjs`、`bash x.sh`)、`node -e` や `python -c` の中の操作 | hooks はコマンドの文字列しか見ない | G.8 の突き合わせ (GitHub の記録に、hooks の記録も控えも対応しない) |
| hooks の無い環境 (別の機器、クラウド、古い版)、hooks が保留されているセッション | hooks が動いていない | SessionStart の合図が無い (エージェントが止まる)。G.8 の突き合わせ |
| オーナーのレビューの後の追加の push | 正当な push と同じ | head を指定したマージが失敗する。監査で、控えの head と PR の head を比べる |
| write 権限を求めるワークフローを足した PR | 正当な CI の変更と区別できない | G.6 の CI の検査で、その PR が不合格になり、マージできない |
| 変数に入れたパスや名前、シンボリックリンク | 文字列から読めない | ハッシュ、期待値、タグの指す先の照合 |
| ブラウザで GitHub を直接操作する (altpaint の外のセッションから) | 対象の判定に掛からない | G.8 の突き合わせ |

## G.6 CI の側の検査

Phase 0 のメタ検査 CI (`meta-check`) に、次の検査を足す。

- `.github/workflows/` のワークフローが、write 権限を求めている (`permissions:` の下の `write`、`write-all`)、または `pull_request_target` か `workflow_run` を使っている場合に、不合格にする。文書だけの PR でも、免除ラベルでも免除しない。
- 目的は、PR のマージやラベルの付与を行うワークフローが main に入らないようにすることである。必須チェックが不合格なので、その PR はマージできない。

8 章 16 が挙げる「オーナー以外が付けた免除ラベルを不合格にする」検査は、入れない。
アカウントが 1 つなので操作者を区別できず、hooks とオーナーの控えで置き換える (8 章 16)。

限界として、`pull_request` で起動する検査は PR の側のファイルで動く。
検査そのものを変える PR は、変えた後の検査で判定される。
オーナーは、`.github/` を変える PR を、マージのスクリプトの表示 (G.7) で見分けてレビューする。

## G.7 オーナーの操作と控え

アカウントが 1 つなので、GitHub の記録では、エージェントの操作とオーナーの操作を区別できない。
区別は、オーナーの操作の控えで行う。
控えは別のファイルにせず、hooks と同じログに `actor: owner` の行として書く。

| 操作 | 方法 | 控え |
| --- | --- | --- |
| PR のマージ | `owner/pr.mjs merge <番号>`。base、head の SHA、ラベル、`.github/` と `.claude/` の変更の有無 (変更ファイルは全件を読む)、チェックの結果を表示し、確認の後に `gh pr merge --merge --match-head-commit <表示した SHA> --delete-branch` を実行する | `owner-merge` (PR の番号、base、head の SHA、head のブランチ名) |
| 免除ラベルの付け外し | `owner/pr.mjs label <番号> <ラベル>`、`owner/pr.mjs unlabel <番号> <ラベル>` | `owner-label` (PR の番号、ラベル、付けたか外したか) |
| ruleset の作成と変更 | `owner/setup-github.mjs`。今の対象と新しい対象、規則が変わるかどうかを表示し、確認の後に実行する。その後に `owner/audit.mjs --init` で期待値を取り直す | 期待値のファイルと、STATUS の「運用の変更」欄 |
| hooks の変更と入れ直し | 置き場所のファイルを直し、`owner/install.mjs` を実行する。このスクリプトは、設定に触れる前に表試験を実行し、通らなければ止まる | ハッシュ (STATUS の「運用の変更」欄)、置き場所の git の履歴 |

- オーナー用のスクリプトは、Claude Code のセッションから起動されると何もせずに終わる (環境変数 `CLAUDECODE` などで見分ける)。置き場所にあるので、エージェントが引数に取るコマンドも hooks が拒否する。
- オーナーは、Web UI のボタンでマージしない。控えが残らず、head の指定も無いからである。控えの無いマージは、監査で「要確認」になる。
- マージ方式は merge commit にする。マージコミットの 2 番目の親が PR の head になり、後から照合できる。

## G.8 追記専用ログと監査

### ログの項目

1 行が 1 回の呼び出しの JSON Lines とする。置き場所は `<G>\log\guard.jsonl` である。
対象と判定した呼び出しは、拒否も許可もすべて書く。対象外の呼び出しは、守る結果 5 に関わる拒否 (`.claude` に触れる読めないコマンドの拒否を含む) だけを書く。複数の節からなるコマンドは 1 行にまとめ、`verb` と `refs` には、push の節があればその節のものを書く (push の節が複数あれば、宛先を合わせる)。

| 項目 | 内容 |
| --- | --- |
| `v`、`ts`、`actor`、`host` | 書式の版、時刻 (UTC、秒まで)、`hook` か `owner`、機器名のハッシュ |
| `g` | 判定のスクリプトのハッシュの先頭 12 桁 |
| `ev`、`sid`、`agent`、`tool`、`tuid`、`mode` | イベント名、セッションの ID、サブエージェントの ID と種類、ツール名、ツール呼び出しの ID、権限モード |
| `tree`、`tgt` | 作業ツリーの種類 (`main`、`worktree`、`outside`)、対象と判定したか |
| `d`、`cls`、`kind` | `allow` か `deny`、分類名、拒否の種類 |
| `verb` | 固定の語彙にした動詞 (`git push`、`gh pr edit` など)。語彙に無いサブコマンドは、`git` か `gh` とだけ書く |
| `refs`、`flags` | push の宛先の ref 名 (5 つまで。使える文字を絞る)、旗 (`force-with-lease`、`delete`、`dry-run`) |
| `pr`、`base`、`labels` | PR の番号、base、既知のラベルの名前 |
| `method`、`endpoint` | `gh api` のメソッドと、端点の種類 (固定の語彙) |
| `path_class`、`skill` | ファイルの種類 (`guard`、`managed`、`user-settings`、`repo-settings`、`credentials`、`workflows`)、スキル名 |
| `sha`、`len` | コマンドの全体の SHA-256 の先頭 16 桁と、長さ |
| `source` | SessionStart の種類 (`startup`、`resume` など) |

秘密が混ざらないように、次のものは書かない。

- コマンドの文字列と、その一部。
- ツールの入力のうち、ファイルの中身、置換の文字列、プロンプト、PR の題と本文。
- 環境変数の値、出力、作業ツリーの外のパス。

コマンドの原文が要るときは、`sid` と `tuid` で transcript を引く。

### 途切れと改変の確かめ方

- 監査のたびに、ログのバイト数と、全体の SHA-256 を控える (`<G>\state\audit.json`)。次の監査で、前回のバイト数までの部分が同じハッシュになることを確かめる。切り詰め、書き換え、削除は、ここで不合格になる。
- すべての行が JSON として読めることを確かめる。
- 今の hooks を入れた時刻より後の行の `g` が、入れた hooks のハッシュと同じであることを確かめる (入れ直す前の行は、前の版で書かれている)。
- hooks が動いていなかった期間は、ログだけからは分からない。次の突き合わせで気づく。
- 行ごとの連鎖 (直前の行のハッシュを持たせる方式) は採らない。並行するツール呼び出しの間で排他が要り、排他の失敗が、この機器のすべてのツール呼び出しを止めるからである。前回の監査より後に書かれた行の書き換えは、この方式では見つけられない。置き場所への書き込みを hooks が拒否していることと、transcript が別に残ることで補う。

### 週次の監査

オーナーが自分の端末で `owner/audit.mjs` を実行する。読み取りだけを行う。

| 検査 | 内容 |
| --- | --- |
| 設定 | G.4 の設定を読み取り、期待値のファイルと丸ごと比べる (ruleset の一覧と詳細、`bypass_actors`、`current_user_can_bypass`、ruleset ごとの最終更新の時刻、main に効いている規則、リポジトリ設定、Actions の権限、コラボレーター、ラベルの一覧)。必須の項目が応答に無い場合は、不合格にする。最終更新の時刻を比べるので、ruleset を変えて戻した場合にも気づく |
| タグ | 保護するタグの ref と SHA が、期待値と同じである |
| 資格情報 | `gh` のアカウントとスコープ、git の `credential.helper`、環境変数、`~/.git-credentials` の有無が、期待値と同じである |
| hooks | 判定のスクリプトと専用の `node.exe` のハッシュ、ユーザー設定の `hooks` の項目のハッシュが、入れたときと同じである。`onFailure: "block"` と、権限の deny の 2 行がある。`disableAllHooks` が、ユーザー設定と、altpaint のすべての作業ツリーの `.claude/settings*.json` に無い。管理設定のファイルとレジストリ (`HKCU` と `HKLM`) が無い |
| ログ | 上の「途切れと改変の確かめ方」。拒否を分類ごとに数えて表示する |
| マージ | 前回の監査より後にマージされた PR のそれぞれに、`owner-merge` の控えがあり、控えの head が PR の head と同じである |
| 免除ラベル | 免除ラベルの付け外しの記録のそれぞれに、`owner-label` の控えがある |
| push (参考) | リポジトリの活動の記録 (ブランチの作成、push、削除) のうち、前後 15 分の範囲に、同じ ref の hooks の許可の行もオーナーの控えも無いものを表示する。8 章 16 が突き合わせを求める対象ではないので、不合格にはしない。hooks の無い環境からの push や、スクリプトの中の push に気づく手掛かりにする |

- マージと免除ラベルで、対応の無い記録は「要確認」として表示し、不合格にする。オーナーが、自分で行った操作かどうかを確かめる。確かめられなければ、逸脱として 8 章 16 の手順で扱う。
- 不合格が無いとき、またはオーナーが確かめて `--accept` を付けたときに、監査の控えを更新する。
- 期待値を保存するとき (`--init`) は、必須の項目 (`allow_auto_merge`、ruleset の `bypass_actors` と `current_user_can_bypass`、Actions の権限の 2 項目) が応答に無ければ、保存せずに止まる。保存した期待値は、置き場所の git の履歴にも記録する。
- リポジトリ設定を変えて戻した場合は、期待値との比較では分からない。GitHub のアカウントの security log には残るが、API では読めない。保持が 90 日なので、月に 1 回、オーナーが画面で、リポジトリの設定の変更 (`repo.` で始まる項目) と、トークンや鍵の追加が無いことを見る。
- 監査にかかった時間は、8 章 16 の定めに従って扱う。

### 同じ拒否が繰り返し出たときの扱い

| 場合 | 見分け方 | 扱い |
| --- | --- | --- |
| 言い換えの再試行 | 1 つのセッションで、種類が「禁止」の拒否が 3 回以上ある (監査が「要確認」として表示する) | オーナーが transcript を見る。拒否された後に別の書き方で同じ結果を得ようとしていたら、規約の周知の問題として、CLAUDE.md と拒否の文面を直す。守る結果は破られていないので逸脱には数えない。8 章 16 の逸脱時の対応 (4) を判断する材料として、付録 H に記録する |
| 誤検知 | 種類が「書き方」の拒否か、禁止の意図が無い呼び出しの拒否が、同じ分類で週に 3 件を超える | 誤検知の是正 (8 章冒頭の細目)。オーナーが判定を直し、実例を表試験に足し、STATUS の「運用の変更」欄に記録する |
| hooks の不具合 | 分類が `internal-error` | 後回しにしない。その日のうちにオーナーが直す |

3 回と 3 件は初期値である。

## G.9 資格情報の置き場所

8 章 16 の「資格情報の置き場所」は、Q7 で別アカウントを使う場合の項目である。
別アカウントを使わない構成では、「オーナーの資格情報を、今の置き場所から動かさず、増やさない」と読み替える。

- 期待値は G.2 の表の値とし、監査で比べる (アカウント 1 つ、スコープ 4 つ、`manager-core`、環境変数なし、`~/.git-credentials` なし)。
- 同じアカウントで、権限を絞ったトークンをエージェント用に発行する案は採らない。8 章 16 の本文が Q7 の 2 つの構成だけを前提にしているからである。
- スクリプトから GitHub を呼ぶときは `gh` のサブコマンドを使い、トークンを取り出さない、と CLAUDE.md に書く。
- GitHub に登録された SSH 鍵、発行済みのトークン、許可したアプリの一覧は、オーナーが設定の時に画面で 1 回確かめる。

## G.10 試験

| 層 | 行う者 | 内容 | 害が出ない理由 |
| --- | --- | --- | --- |
| T0 判定の表試験 | 導入のスクリプト (自動)、オーナー | 判定に、入力の形をした JSON を渡し、期待した判定と分類を比べる。約 370 例。本物の名前 (main、`legacy-final`、`size-exempt`) を使う。下書きの段階で、Bash と PowerShell の実例 827 件による反証試験にかけ、普通の書き方での見逃しと誤検知を直して、表試験に足した。実例は置き場所の `tests/corpus/` に置き、判定を変えたときに、見逃しと誤検知の候補の数を見比べる (この時点で、見逃しの候補が 5、誤検知の候補が 19。どちらも普通でない書き方か、設計どおりの拒否である) | コマンドを実行しない。判定は入力だけで決まる |
| T1 試し打ち | エージェント (オーナーの指示で) | hooks がツールごとに動くことを確かめる。Bash、PowerShell、Edit、Read、アプリのツール、サブエージェント、Workflow を 1 回ずつは通す。手順と表は、置き場所の `tests/T1.md` | `--dry-run`、`--help`、存在しない対象、ファイルに無い文字列の置換だけを使う |
| T2 GitHub の側の試験 | オーナー (`owner/test-github.mjs`) | hooks を通らなかった場合に GitHub が止めるものを、ダミーで確かめる。「拒否された」の判定は、終了コードに加えて、拒否の理由の文面でも確かめる (通信や認証の失敗を合格にしない) | 名前が `zz-test` で始まるダミー (ブランチ、タグ、ruleset、PR) にだけ書き込み、最後に片付ける。閉じたダミーの PR は GitHub から消せないので残る |
| T3 監査の確認 | オーナー (`owner/audit.mjs`) | T1 の拒否と、T2 のマージとラベル付与が、監査で見えることを確かめる | 読み取りだけ |

前提として、次のものが main に入っていること。G.6 の検査と、docs/ACCEPTANCE.md (ID の参照の検査は、このファイルがあるときに働く)。T2 のスクリプトは、最初の読み取りでこの 2 つを確かめ、無ければ書き込みの前に止まる。CLAUDE.md の hooks についての決まり (G.12) は、hooks を入れて T1 を終えた後の PR で足す。先に足すと、合図が無いあいだ、エージェントが push できなくなるからである。

順序は、hooks を入れる、ruleset を入れる、設定を読み取って期待値として保存する、T2、T1、T3 とする。
本物の設定は、T2 の最初に読み取りで確かめる (8 章 16 の試験の原則)。

T1 の最初の 1 件 (`git push --dry-run origin HEAD:main` の拒否) が通らなければ、hooks が動いていない。残りの試験を行わずに止める。
T1 の合格の基準は、表のすべての行が期待どおりになることである。期待と違った行があれば、G.2 の「期待と違った場合」の扱いを当てるか、判定を直してから、T1 を最初からやり直す。

T2 が確かめることは、次のとおりである。ダミーの ruleset は、本物の ruleset の本文から名前と対象だけを差し替えて作るので、同じ規則を試すことになる。

| 守る結果 | 確かめること |
| --- | --- |
| 1 | ダミーの base への直接 push と削除を、GitHub が拒否する |
| 1 | 行数の上限を超える PR、変更ファイル数の上限を超える PR、ID を参照しない PR、write 権限を求めるワークフローを足す PR を、メタ検査 CI が不合格にする。不合格の PR は、`--admin` を付けてもマージできない |
| 1 | PR に追加の push があると、前の head を指定したマージが失敗する。自動マージを設定できない (必須チェックが不合格の PR で試す) |
| 1 | 合格した PR は、head を指定してマージできる (規則が正しいマージを止めていない) |
| 2 | ダミーのラベルの付与とマージが GitHub の記録に残り、控えと対応する (T3 で見る) |
| 4 | ダミーのタグ (入れ子の名前を含む) の削除と強制更新を、GitHub が拒否する |

試験の途中で hooks を入れ直した場合 (サブエージェントの拒否を入れた、文面を直した、など) は、T1 を最初からやり直してから T3 を行う。T2 はやり直さない。

T3 の合格の基準は、次の 3 つである。監査の不合格が 0 件である (設定、タグ、資格情報、hooks のハッシュの照合を含む)。T1 の表の拒否の分類が、監査の表示する拒否の集計に出ている。T2 のマージ 1 件とラベルの付与 1 件が、「要確認」なしで控えと対応している。

ダミーのラベル `zz-test-exempt` だけは、片付けずに残す。試験をやり直せるようにするためと、消すとラベル付与の記録を T3 で確かめられなくなるおそれがあるためである。メタ検査 CI はこのラベルを免除として扱わないので、付いても何も免除されない。hooks は、本物の免除ラベルと同じく拒否する。

「付録 G の設定を終えた時点」(8 章 16 が効き始め、監査が始まる時点) は、T3 に合格し、ハッシュと期待値を STATUS に記録し、hooks の決まりを CLAUDE.md に足す PR を、オーナーが `owner/pr.mjs` でマージした時点とする。

守る結果 3、5、6 は、GitHub の側に止める仕組みが無い。T0 と T1 で hooks の層を確かめ、T3 で設定とハッシュの照合を確かめる。

8 章 16 が認める例外 (本物の main への直接 push、リポジトリ設定の現在値の再設定) は、行わない。
ダミーのブランチで同じ規則の本文を試し、main に同じ規則が効いていることを `rules/branches/main` の読み取りで確かめれば足りる。
本物の main への push は、通ってしまった場合に、保護を外さないと取り除けない。

試験をやり直すのは、hooks を変えたとき (T0、T1)、別の機器に入れたとき (T0、T1)、保護の対象や必須チェックを変えたとき (T2、期待値の更新) である。
SessionStart の合図が出なくなったときも、T1 をやり直す。

## G.11 決めたことの一覧

8 章 16 がこの付録に委ねた項目は、次のとおり決めた。

| 項目 | 決定 | 章 |
| --- | --- | --- |
| 具体的な設定値 | ruleset 2 本と、リポジトリ設定の期待値 | G.4 |
| hooks のパターン | 引用符を読む字句解析と、`git` と `gh` の引数への規則 | G.5 |
| CI の検査 | write 権限を求めるワークフローの不合格。免除ラベルの操作者の検査は入れない | G.6 |
| 監査のスクリプト | `owner/audit.mjs` | G.8 |
| 試験の手順 | T0〜T3 | G.10 |
| 資格情報の置き場所 | 動かさず、増やさない | G.9 |
| ログに記録する項目 | 分類と固定の語彙。コマンドの文字列は書かない | G.8 |
| ログが途切れず改変されていないことの確かめ方 | 前回の監査までの部分のハッシュ、GitHub の記録との突き合わせ | G.8 |
| `spike-base` を本物で試すかダミーで試すか | 書き込みはダミー。本物は読み取りと、hooks の拒否の確認だけ | G.4 |
| 同じ拒否が繰り返し出たときの扱い | 3 つに分けて扱う | G.8 |
| サブエージェントに hooks が効かない場合 | 起動するツールを拒否するスイッチを入れる | G.5 |

採らなかった案は、次のとおりである。

| 案 | 採らなかった理由 |
| --- | --- |
| hooks を管理設定 (`managed-settings.json`) に置く | 8 章 16 が「ユーザー設定に置く」と定めている。project の `disableAllHooks` で止まる弱点は、altpaint の設定ファイルへの書き込みの拒否と、監査で補う |
| git の pre-push フックを補助に入れる | 守る対象の場所が増える。スクリプトの中の push は、監査の突き合わせで気づく |
| ユーザー設定の `Bash(gh pr:*)`、`Bash(gh auth:*)` の許可を外す | ほかのプロジェクトの作業に影響する。hooks は権限の規則より前に動き、失敗したときは止まる側に倒してある |
| `.github/workflows/` への Write を、内容を見て拒否する | Bash での書き込みには効かない。CI の検査 (G.6) で不合格にする |
| スクリプトのファイルの中身を読んで判定する | 言語ごとの解析が要る。想定する相手に対しては、拒否の文面で「スクリプトに移さない」と伝えることと、監査で足りる |
| MCP のツールを、許可するサーバーの一覧で絞る | サーバーの名前が識別子で、一覧の維持が難しい。名前に `github` を含むツールと、G.5 の表のツールを拒否する |
| ログの行ごとの連鎖 | G.8 のとおり |
| マージ方式を ruleset で merge commit に限る | オーナーのスクリプトが merge commit でマージするので足りる |

## G.12 この付録の後の正本

| 内容 | 正本 |
| --- | --- |
| hooks の判定、表試験、T1 の手順、オーナー用のスクリプト、ruleset の本文、期待値 | 置き場所 (`%USERPROFILE%\.altpaint-guard`) のファイル。オーナーが直接変える |
| hooks のハッシュ、期待値のハッシュ、入れ直す手順 | docs/STATUS.md の「運用の変更」欄と「再開手順」欄。「再開手順」には、入れ直しのコマンドの列 (控えを置き場所へ写す、`owner/install.mjs` を実行する、T1 を行う、表示されたハッシュを「運用の変更」欄と比べる) を、秘密の値を含まない形で書く |
| CI の検査 | `.github/workflows/meta-check.yml` と `.github/scripts/meta_check.mjs` |
| エージェントが守る決まり (合図が無いセッションでの操作の禁止、ブランチ名に使わない接頭辞、トークンを取り出さないこと、禁止の一覧) | CLAUDE.md |

判定の規則を変えるのは細目であり、オーナーが置き場所のファイルを直接変えて、STATUS に記録する (8 章冒頭)。
照合を弱める変更は「緩め」であり、ADR 020 の改訂の手続きで行う (同)。
