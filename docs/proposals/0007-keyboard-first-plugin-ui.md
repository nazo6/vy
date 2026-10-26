---
id: "0007"
title: "キーボードファーストなプラグイン UI の実装計画"
type: "proposal"
status: "open"
date: 2026-10-27
related_adrs: ["0003", "0005", "0006", "0022", "0024"]
tags: ["ui", "keymap", "focus", "plugin"]
---

# [Proposal] キーボードファーストなプラグイン UI の実装計画

## 1. 決定済みの前提

決定は [ADR 0024](../adr/0024-keyboard-first-plugin-ui.md) にあります。この提案は、その決定を実装するための規範の詳細と作業計画です。

- プラグインに生のキーは渡さない（[ADR 0005](../adr/0005-native-plugin-ui.md)）。
- キーはキーマップがコマンドへ解決し、フォーカス中のウィジェットがアクションとして受け取る。
- ペイン間のフォーカスはコア、ペイン内のフォーカスはクライアントが持つ（[ADR 0003](../adr/0003-editor-state.md)）。
- アクションはコマンド ID で表し、実行キューとケイパビリティ検査を通す（[ADR 0022](../adr/0022-command-surfaces.md)）。
- `views.register` の `render` / `restore` は関数ではなくコマンド ID にする。

**この提案に書かれた「〜する」は、ADR 0024 の決定を実装するための規範です。** 新しい設計判断が必要になったら、先に ADR を追加します。

## 2. 規範

### 2.1 キーマップのコンテキスト

`when` から参照できる値を次のとおり定めます。すべてホストが設定し、プラグインは同じ名前を上書きできません。プラグイン独自の条件は `keymap.setContext` で別の名前を足します。

| コンテキスト       | 値                                                                           | 設定する層   |
| :----------------- | :--------------------------------------------------------------------------- | :----------- |
| `mode`             | `normal`、`insert`、`select` 系                                              | クライアント |
| `paneKind`         | `editor`、`ui`、`webview`、`terminal`                                        | クライアント |
| `viewType`         | ペインが開いているビューの ID（エディタでは空）                              | クライアント |
| `uiRole`           | `list`、`tree`、`table`、`textInput`、`button`（フォーカスが無ければ未設定） | UI ホスト    |
| `uiFocus`          | フォーカス中のノードの `id`                                                  | UI ホスト    |
| `uiSelectionCount` | 選択中の項目数                                                               | UI ホスト    |
| `overlay`          | `none`、`palette`、`picker`、`ex`、`prompt`、`approval`、`settings`          | クライアント |

`uiRole` と `uiFocus` は、フォーカス可能なウィジェットにフォーカスがあるときだけ設定します。エディタペインでは未設定なので、`uiRole == 'list'` のような条件は自動的に偽になります。

### 2.2 ペイン内フォーカス

- クライアントは、ペインごとに `FocusPath`（ルートからフォーカス中のノードまでの `id` の列）を持ちます。ペイン間のフォーカスはコアが持つという ADR 0003 の分担を変えません。
- フォーカス可能なノードは `list`、`tree`、`table`、`textInput`、`button` です。それ以外のコンテナとテキストはフォーカスを持ちません。
- `Tab` と `Shift-Tab` は、アクティブペイン内のフォーカス可能なノードを深さ優先の順で移動します。マウスのクリックも同じフォーカスを設定します。
- 再描画でフォーカス中のノードが消えたときは、最も近い生存する祖先へ移り、それも無ければ最初のフォーカス可能なノードへ移ります。
- フォーカスはペインごとに保持し、スクロール位置と同じく `session.json` に保存します。木そのものは保存しません（[`ui.md`](../specs/ui.md) §5）。

### 2.3 アクションの語彙と既定キー

プラグイン UI の操作は、閉じた集合のコマンド `vy.ui.*` で表現します。既定キーをホストが提供し、利用者が同じコマンドを別のキーへ割り当てられるようにするためです。

| コマンド          | アクション名 | 既定のキー            | list / table               | tree                       | textInput                  |
| :---------------- | :----------- | :-------------------- | :------------------------- | :------------------------- | :------------------------- |
| `vy.ui.next`      | `next`       | `j`、`Down`、`Ctrl-n` | カーソルを次へ             | 次のノードへ               | なし                       |
| `vy.ui.prev`      | `prev`       | `k`、`Up`、`Ctrl-p`   | カーソルを前へ             | 前のノードへ               | なし                       |
| `vy.ui.pageDown`  | `pageDown`   | `Ctrl-d`              | 1 ページ進む               | 1 ページ進む               | なし                       |
| `vy.ui.pageUp`    | `pageUp`     | `Ctrl-u`              | 1 ページ戻る               | 1 ページ戻る               | なし                       |
| `vy.ui.first`     | `first`      | `g g`                 | 先頭へ                     | 先頭へ                     | なし                       |
| `vy.ui.last`      | `last`       | `G`                   | 末尾へ                     | 末尾へ                     | なし                       |
| `vy.ui.expand`    | `expand`     | `l`                   | なし                       | 展開する                   | なし                       |
| `vy.ui.collapse`  | `collapse`   | `h`                   | なし                       | 折りたたむ                 | なし                       |
| `vy.ui.activate`  | `activate`   | `Enter`               | 決定する                   | 決定する                   | 送信する（1 行）           |
| `vy.ui.cancel`    | `cancel`     | `Esc`                 | 一時状態を戻す             | 一時状態を戻す             | insert を抜ける            |
| `vy.ui.toggle`    | `toggle`     | `x`                   | 選択をマークする           | 展開を切り替える           | なし                       |
| `vy.ui.yank`      | `yank`       | `y`                   | 選択項目のテキストをコピー | 選択項目のテキストをコピー | なし                       |
| `vy.ui.focusNext` | `focusNext`  | `Tab`                 | 次のフォーカス可能ノードへ | 次のフォーカス可能ノードへ | 次のフォーカス可能ノードへ |
| `vy.ui.focusPrev` | `focusPrev`  | `Shift-Tab`           | 前のフォーカス可能ノードへ | 前のフォーカス可能ノードへ | 前のフォーカス可能ノードへ |

- `activate` は `args.action` で別のアクション名に展開できます。たとえば `Ctrl-s` を `activate` と `action: "split"` に割り当て、プラグインは分割オープンを実装できます。
- `cancel` は一時状態（検索語、マーク、プレビュー、展開の一時変更）を戻します。戻すものが無いときは「処理しなかった」とみなし、`Esc` のフォーカス層（2.8）へ委ねます。
- `toggle` の意味は役割で決めます。ツリーでは展開、一覧と表ではマークです。
- `yank` は項目の `text` を unnamed レジスタへコピーします。`text` が無い項目は `label` を使います。
- `button` は `activate` で対応するコマンドを実行します。ボタンにフォーカスがある間の `Enter` が既定です。
- `vy.ui.*` はパレットに表示せず、Ex 名も持ちません（[`commands.md`](../specs/commands.md) §4.1 の規則）。

### 2.4 アクションの解決と引数

1. キーマップがキー列をコマンドへ解決します。
2. `vy.ui.*` のコマンドは、正規のアクション名（`next`、`activate` など）か `args.action` をアクション名にします。
3. フォーカス中のウィジェットを含むビューが、そのアクション名をコマンド ID に対応づけていれば、そのコマンドを実行します（プラグインによる上書き）。
4. 対応づけが無ければ、ホストが役割ごとの既定動作を実行します。

プラグインのコマンドには、キーマップの引数に加えて次の `UiActionArgs` を渡します。プラグインはフォーカスや選択を問い合わせずに操作できます。

| フィールド    | 内容                                           |
| :------------ | :--------------------------------------------- |
| `action`      | 実行されたアクション名                         |
| `viewId`      | ビューの ID                                    |
| `paneId`      | ペインの ID                                    |
| `role`        | `list`、`tree`、`table`、`textInput`、`button` |
| `focusId`     | フォーカス中のノードの `id`                    |
| `selectedIds` | 選択中の項目の `id`（単一選択でも 1 件入る）   |
| `anchorId`    | カーソル位置の項目の `id`                      |
| `expandedIds` | ツリーで展開中のノードの `id`                  |
| `depth`       | ツリーでの深さ                                 |
| `query`       | picker の検索語                                |
| `count`       | キーマップのカウント                           |
| `register`    | キーマップのレジスタ                           |

ホストの既定動作はクライアントの Rust で完結させます。キーを押すたびにコマンドを実行すると、実行キューとタイムアウト（[`plugin-system.md`](../specs/plugin-system.md) §2.2）を通るため、連続した移動が遅れます。

同じ情報をコマンド以外の入口（パレット、Ex、マウス、スクリプト）からも参照できるよう、`ui.focused()` を用意します。エディタペインでは `null` を返します。

### 2.5 ビューの宣言

`views.register({ id, title, kind, singleton, preferredSize, actions, restore })` は宣言だけを行います。`actions` はアクション名からコマンド ID への対応、`restore` はコマンド ID です。

- 木はプラグインが `views.render({ id, tree })` で push します。ホストは前回の木との差分を取り、変わったノードだけを描きます（[`ui.md`](../specs/ui.md) §3）。
- 復元では、ホストがコアから受け取った `params` と `state` を `restore` コマンドへ渡します。そのあとプラグインが `views.render` を呼びます。
- アクションと復元がコマンドであることで、ケイパビリティ検査と実行キューを既存の 1 本の経路に保てます。

### 2.6 既定キーとバインディングの条件

既定キーマップの条件を次のように分けます。

| 種類          | 条件の例                                    | 対象                                   |
| :------------ | :------------------------------------------ | :------------------------------------- |
| エディタ      | `paneKind == 'editor' && overlay == 'none'` | 移動、編集、選択、insert のキー        |
| プラグイン UI | `uiRole == 'list'`、`uiRole == 'tree'`      | `vy.ui.*` の既定キー                   |
| グローバル    | 条件なし                                    | `Ctrl-w` 系のペイン操作、`:`、リーダー |

- 同じキー列が複数一致した場合は、完全一致が即座に確定し、条件が同じなら後から登録したものが優先されます（現行の規則）。条件を `paneKind` と `uiRole` で排他にするため、具体性のランキングやビュー単位のスコープ層は導入しません。
- プラグインがキーを足すときは、`viewType`、`uiRole`、`uiFocus` のいずれかを条件に含めます。条件を含めないバインドは、すべての場所で一致します。
- `space` はリーダー専用にします。単独の `space` にパレットを割り当てるのをやめ、`space space` をパレットにします。これにより `space f f` のような利用者定義が成立します。既存の利用者にとっては破壊的変更です。
- `Enter` と `Esc` は、フォーカス中のウィジェットの役割に応じた既定動作（送信、insert の終了、一時状態の解除）を行います。役割に既定動作が無ければグローバルへ流れます。

### 2.7 picker とオーバーレイ

- picker はペインではなくオーバーレイのままです（[`panes.md`](../specs/panes.md) §2）。枠、入力欄、一覧、プレビューはホストが描きます。
- picker のキーもキーマップを通します。ホストの固定 `match` 文で処理するのをやめ、`overlay == 'picker'` と `uiRole == 'list'`、`uiFocus`（picker の `id`）で既定キーを引きます。これによりプラグインは `Ctrl-s` などの独自のキーを割り当てられます。
- picker は insert モードで開きます。検索語の入力はテキストエンジンが扱い、`Esc` で normal モードへ、normal で再度 `Esc` を押すと閉じます。normal では一覧の既定キー（`j`、`k`、`g g`、`G`、`Ctrl-d`、`Ctrl-u`、`Enter`、`x`、`y`）が使えます。
- `ui.picker.show({ id?, items?, fetch?, multiple?, preview?, placeholder? })` は `{ action, selectedIds }` または `null` を返します。`Enter` は `action: "default"`、`vy.ui.activate` に `args.action` を付けたキーはその名前を返します。`multiple` が真なら `selectedIds` に複数入ります。
- `fetch` はコマンド ID です。検索語が変わったとき、デバウンスして `{ query, requestId }` で実行し、プラグインは `ui.picker.update({ items })` で結果を差し替えます。`items` を渡した場合はクライアント側で絞り込み、コマンドの往復をしません。
- `preview` はウィジェット木です。画像、コード、テキストを既存の部品で表示します。Webview は使いません（ADR 0005）。
- `ui.picker.update` と `ui.picker.close` を追加します。

### 2.8 Esc のフォーカス層

`Esc` の優先順位を次の 5 層に確定します。上から順に評価し、最初に該当した層だけを処理します。

1. オーバーレイが開いている: picker は insert から normal へ、normal の picker とその他のオーバーレイは閉じて直前のフォーカスへ戻す。
2. エディタペインが insert または select 系: normal へ戻し、選択を縮小する。
3. プラグイン UI に一時状態がある、またはビューが `cancel` を上書きしている: `vy.ui.cancel` を実行して止める。ホストの既定動作が「戻すものが無い」と判断した場合だけ次へ進む。
4. プラグイン UI にフォーカスがある: 最後にフォーカスしたエディタペインへ戻す。
5. エディタの normal: ホバーなどの一時表示を閉じる。無ければ何もしない。

## 3. 未決定の論点

- oil.nvim のような編集可能な一覧の方式（仮想ドキュメントか、一覧のインライン編集か）。
- UI ペインのステータスラインとモード表示（[ADR 0006](../adr/0006-user-controlled-ui-and-slots.md) の `ui.statusline.mode` と連動）。
- 一覧とツリーでの範囲選択（`v` による視覚選択、テキストオブジェクトに相当する操作）。
- フローティングペインと、プラグインが独自に開くオーバーレイ（[`ui.md`](../specs/ui.md) §8 の制約の再検討）。
- picker の非同期検索のキャンセルと、古い結果を破棄する規則（`requestId` の扱い）。
- プラグイン固有のアクション名を `actions` に許すか、閉じた語彙の上書きだけにするか。

## 4. 作業計画

### Phase 1: コンテキストとペイン内フォーカス

- `when` のコンテキストに `paneKind`、`viewType`、`uiRole`、`uiFocus`、`uiSelectionCount`、`overlay` を追加する（`crates/vy-app/src/when.rs`、`keymap.rs`）。
- ペインごとの `FocusPath` と、再描画時のフォールバックを実装する（`crates/vy-app/src/ui/`、`view/render.rs`）。
- `Tab` / `Shift-Tab` のフォーカス移動を、ウィジェット木の DFS 順で実装する。
- `builtin/keymaps/default.json` のエディタ系バインドに `paneKind == 'editor'` を付ける。

### Phase 2: `vy.ui.*` の語彙と既定キー

- `vy.ui.*` のコマンドを `crates/vy-client-api` のコマンド定義へ追加する（`palette: hidden`、Ex 名なし）。
- 一覧・ツリー・表の既定動作をクライアントの Rust に実装する。移動と選択はコマンド実行キューを通さない。
- `UiActionArgs` をプラグインのコマンドへ渡す経路を作る。
- `ui.focused()` を追加する。

### Phase 3: picker のキーマップ化

- `ui.picker.show` の固定キー処理を、`overlay == 'picker'` の既定キーへ移す。
- `ui.picker.update`、`ui.picker.close`、`requestId` による結果の破棄を実装する。
- picker を insert / normal の 2 状態で動かす。

### Phase 4: views の actions / restore

- `views.register` の `render` / `restore` をコマンド ID に変える。ホストから JS の関数を呼ぶ経路は作らない。
- `views.render` の差分適用と、フォーカス中のノードが消えたときのフォールバックを実装する。

### Phase 5: `space space` への移行

- 既定キーマップの `space` をリーダーだけにし、`space space` をパレットに割り当てる。
- `docs/specs/keymap.md` のキー表と README の操作表を更新し、移行を案内する。

### 対象ファイル（見込み）

- `crates/vy-app/src/keymap.rs`、`when.rs`、`view/{mod,input,render,commands}.rs`、`ui/{pane,list,tree,widget,picker}.rs`
- `crates/vy-client-api/src/command.rs`、`ui.rs`（`UiActionArgs`、`views`、`ui.focused`）
- `builtin/keymaps/default.json`
- `docs/specs/{ui,keymap,commands}.md`、`docs/adr/0024-keyboard-first-plugin-ui.md`

## 5. 仕様書への反映（実装完了時）

この提案の規範は、実装が入った時点で次の仕様書へ移し、proposal は `status: "closed"` にします。

- `docs/specs/ui.md` §2（コンテキスト、アクションの語彙、解決順）、§9（フォーカスと Esc 層）
- `docs/specs/keymap.md`（コンテキストの一覧、`space space`、`vy.ui.*` の既定キー）
- `docs/specs/commands.md` §4.1（hidden の規則に `vy.ui.*` を追記）
- `docs/specs/panes.md`（picker のキー経路）

## 6. 完了条件

- UI ペインで `j` / `k` / `gg` / `G` / `Ctrl-d` / `Ctrl-u` / `Enter` / `x` / `y` / `Esc` がエディタと同じ意味で動く。
- 利用者が `keymap.bind` で `vy.ui.*` を別のキーへ割り当てられる。
- `space f f` と `space space` が共存する。
- 上記の仕様書への反映が完了し、ADR 0024 の `implementation` が `done` になる。
