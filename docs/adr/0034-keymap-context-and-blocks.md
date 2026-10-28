---
id: "0034"
title: "キーマップコンテキストの直交化とブロック構造への再構成"
type: "adr"
status: "accepted"
category: "maintenance"
date: 2026-10-28
tags: ["keymap", "refactor", "context", "when"]
supersedes: []
superseded_by: null
related_adrs: ["0024", "0029", "0031", "0032"]
---

# ADR 0034: キーマップコンテキストの直交化とブロック構造への再構成

- 状態: 承認（実装完了）
- 日付: 2026-10-28
- カテゴリー: maintenance
- 実装: 完了

## 文脈

`builtin/keymaps/default.json` は 147 件のバインディングを平坦な配列で持ちます。各要素が `keys`・`command`・`when` を 1 組持つため、条件がキーごとに繰り返されています。ADR 0024 と ADR 0032 でコンテキスト（`paneKind`、`overlay`、`uiRole`、`mode`）を導入した結果、この繰り返しと複雑化が顕在化しました。

現状を計測した結果は次のとおりです。

| 指標                            | 値       | 補足                                                              |
| :------------------------------ | :------- | :---------------------------------------------------------------- |
| バインディング数                | 147      | 1 行 5 行（整形後は平均 5 行）                                    |
| ファイル行数                    | 736      |                                                                   |
| 異なる `when` 文字列            | 17       | 実際の条件は 10 ブロック程度に収まる                              |
| 最頻条件の反復                  | 44 回    | `paneKind == 'editor' && overlay == 'none' && (mode != 'insert')` |
| 同一キー表の複製                | 1 組     | `ui.list` = `overlay.list`（13 キー）                             |
| 複数箇所に散らばる同一コマンド  | 11 種    | `vy.ui.next` は 4 条件・12 箇所                                   |
| Insert 以外で `uiRole` を書く量 | 115 字符 | 5 役割の否定列挙が 11 回                                          |

当初の起草案では、この冗長さを解消するために「プロファイル（キー表の辞書）」と「スコープ（継承関係付きの条件）」という 2 層のマクロ・継承 DSL を JSON 側に導入しようとしました。しかし、根本原因を分析すると、問題は「JSON のマクロ機能の不足」ではなく、**コンテキストの非直交性とブール代数の未整理** にありました。

具体的には次の 3 つの構造的課題が存在します。

1. **コンテキストの背面漏れ（非直交性）**:
   モーダルなサーフェス（パレットや Ex コマンドラインなど）が開いている間も、クライアントの `KeyContext` には背面ペインの `paneKind` や `uiRole` がそのまま残ります。そのため、ほぼすべてのエディタ・UI キーに `&& overlay == 'none'` を防衛的に書き足す必要が生じていました。また、オーバーレイ自身がウィジェット役割（`uiRole`）を名乗っていなかったため、「UI ペインの一覧」と「オーバーレイの一覧」で全く同じキー表（13 キー）を別条件として二重定義していました。
2. **論理式の過剰分割と未整理**:
   UI ペインの移動キー（`vy.ui.next` 等）は、`list` / `tree` / `table`、`button`、および「未設定（5 役割すべての否定列挙）」の 3 か所に過剰分割されていました。`uiRole` の取りうる状態（5 役割＋未設定）から見れば、これは論理的に **`paneKind == 'ui' && uiRole != 'textInput'`** の 1 つの条件にすぎません。また、`when.rs` は識別子が未設定（`None`）のとき偽として評価するため、5 役割の肯定は `uiRole`、否定は `!uiRole`、テキスト入力以外は `uiRole != 'textInput'` で直ちに表現可能でした。
3. **JSON への過剰な抽象化（独自 DSL）の持ち込み**:
   本来コンテキストの直交化とブール代数の整理で解くべき問題を、JSON 側のマクロ機能（`profiles` / `extends` / `noRole` / スコープ名述語）で解こうとすると、間接参照のホップ数が増え、プロトタイプ継承の曖昧さ（条件の継承かキーの継承か）や順序依存が残り、設計としてクリーンになりません。

ADR 0024 の「既定キーは 1 か所（`builtin/keymaps/default.json`）に置き、ヒント文字列もそこから生成する」という原則を維持しつつ、**コンテキストの直交化** と **単純明快な単層ブロック構造** によってキーマップを再構成します。

## 決定ドライバ

1. **直交性と明示性**: キーボードフォーカスは常に最前面の 1 つである原則（ADR 0024 / ADR 0032）をコンテキストに反映し、背面の状態が漏れないこと。
2. **直感的な述語**: `editorFocus`、`modal`、`hasSelection` などの意味述語を活用し、冗長な文字列比較や `||` の連打を排除すること。
3. **単層構造（間接参照ゼロ）**: プロファイル定義や継承ツリーなどの多層マクロを作らず、Zed のような `[{ when, bindings }]` の単層ブロック構造にすること。
4. **完全な順序非依存**: ブロック間・キー間の暗黙のシャドウイング（`ctrl-u` の二重定義や pending モードの後勝ち）を排他的な条件で明示化し、ファイルの並び順に依存しないこと。
5. **挙動不変**: 実際に到達しうるすべての実行時コンテキストにおいて、キー解決結果を 1 件も変えないこと。
6. **ユーザー拡張性との一致**: 内部実装専用の複雑な DSL を作らず、将来ユーザーが設定するキーマップ形式（`~/.config/vy/keymap.json`）とも自然に統一できる形式であること。

## 検討した選択肢

| 選択肢                                                    | 内容                                                                      | 評価                                                                                      |
| :-------------------------------------------------------- | :------------------------------------------------------------------------ | :---------------------------------------------------------------------------------------- |
| A. 平坦配列のまま条件のみ簡約                             | JSON 構造は変えず、`editorFocus` や `uiRole != 'textInput'` で記述を整理  | 記述量は減るが、同じ条件の反復（44 回）と行数の多さ（700 行超）が残る                     |
| B. プロファイルとスコープの 2 層 DSL（旧 ADR34）          | キー表を `profiles` に辞書化し、スコープから参照・継承する                | 行数は減るが、間接参照が 2 段になり、プロトタイプ継承の解釈が複雑。根本原因に届いていない |
| C. コンテキスト直交化 ＋ 単層コンテキストブロック（採用） | フォーカスを最前面 1 つに直交化し、単層の `{ when, bindings }` 配列にする | 概念が 1 つだけで間接参照ゼロ。キーの重複が完全に消え、行数も約 210 行に激減する          |
| D. 式言語を廃止し Rust 定義にする                         | すべて Rust の静的テーブルにする                                          | 最も型安全だが、`init.ts` からの動的拡張や将来の JSON カスタマイズと分断される            |

## 決定

### 1. フォーカスターゲットの単一化とコンテキストの直交化

ADR 0024 §3 および ADR 0032 §5 の基本原則「**キーボードのフォーカスは常に 1 つ**（モーダルなサーフェスが開いていればそのサーフェス、無ければアクティブペイン）」を `KeyContext` の生成（`crates/vy-app/src/view/mod.rs`）に徹底します。

1. **モーダルサーフェス表示時の背面マスク**:
   モーダルなサーフェス（`overlay.is_modal()` が真）が開いている間は、サーフェス自身がフォーカスターゲットとなります。背面の `paneKind` やウィジェットの `uiRole` は `KeyContext` に露出させず、マスクします（`editorFocus` は自動的に `false` となり、`paneKind` は `overlay` 等として扱われます）。
2. **サーフェス自身による `uiRole` の提供**:
   開いているサーフェスは、自身が持つウィジェットの役割を `uiRole` として報告します。
   - `palette` / `picker` / `settings`（一覧選択モード）：`uiRole = 'list'`
   - `ex` / `prompt` / `settings`（テキスト入力モード）：`uiRole = 'textInput'`
   - `approval`：`uiRole = 'button'`
     これにより、「UI ペインの一覧」と「オーバーレイの一覧」はどちらも同じ `uiRole == 'list'` で照合され、キー表の二重定義（`ui.list` と `overlay.list`）が完全に不要になります。

### 2. 意味述語の整備

`when` 式で直感的に使えるよう、以下の意味述語を `KeyContext` の計算プロパティとして提供します。

| 述語           | 意味                                                               | 判定内容                                                                            |
| :------------- | :----------------------------------------------------------------- | :---------------------------------------------------------------------------------- |
| `editorFocus`  | エディタペインが入力を専有しているか                               | `overlay == 'none' && paneKind == 'editor'`                                         |
| `modal`        | モーダルサーフェスが開いているか                                   | `overlay.is_modal()`（`palette`, `picker`, `ex`, `prompt`, `approval`, `settings`） |
| `hasSelection` | 選択項目を持つウィジェット（一覧・ツリー・表）にフォーカスがあるか | `uiRole == 'list' \|\| uiRole == 'tree' \|\| uiRole == 'table'`                     |
| `normalMode`   | ノーマルモードか                                                   | `mode == 'normal'`                                                                  |
| `insertMode`   | インサートモードか                                                 | `mode == 'insert'`                                                                  |
| `selectMode`   | セレクト（ビジュアル）系モードか                                   | `mode == 'select' \|\| mode == 'select_line' \|\| mode == 'select_block'`           |

`uiRole` が未設定の状態は、`when.rs` の既存の言語仕様どおり `!uiRole` で表現します（専用述語 `noRole` は導入しません）。

### 3. 単層コンテキストブロック構造（`ContextBlock`）

`builtin/keymaps/default.json` の構造を、間接参照やマクロを持たない **単層のコンテキストブロック配列** に改めます。

```jsonc
[
  // 1. グローバル（無条件）
  {
    "bindings": {
      "esc": "vy.mode.normal",
    },
  },

  // 2. ノーマルモード共通（ペイン操作、パレット起動）
  {
    "when": "normalMode && !modal",
    "bindings": {
      "space": "vy.ui.command_palette",
      "ctrl-w h": "vy.pane.focus_left",
      "ctrl-w j": "vy.pane.focus_down",
      "ctrl-w k": "vy.pane.focus_up",
      "ctrl-w l": "vy.pane.focus_right",
      "ctrl-w v": "vy.pane.split_right",
      "ctrl-w s": "vy.pane.split_down",
      "ctrl-w q": "vy.pane.close",
      "ctrl-w w": "vy.pane.next",
      "ctrl-w +": "vy.pane.increase_height",
      "ctrl-w -": "vy.pane.decrease_height",
      "ctrl-w >": "vy.pane.increase_width",
      "ctrl-w <": "vy.pane.decrease_width",
      "ctrl-w =": "vy.pane.equalize",
    },
  },

  // 3. Ex コマンドラインの起動
  {
    "when": "editorFocus && !insertMode",
    "bindings": {
      ":": "vy.ui.ex_command_line",
    },
  },

  // 4. エディタ基本操作（移動・編集・モード遷移）
  {
    "when": "editorFocus && !insertMode && !textObjectPending",
    "bindings": {
      "h": "vy.motion.left",
      "left": "vy.motion.left",
      "l": "vy.motion.right",
      "right": "vy.motion.right",
      "j": "vy.motion.down",
      "down": "vy.motion.down",
      "k": "vy.motion.up",
      "up": "vy.motion.up",
      "w": "vy.motion.word_forward",
      "b": "vy.motion.word_backward",
      "e": "vy.motion.word_end",
      "0": "vy.motion.line_start",
      "home": "vy.motion.line_start",
      "$": "vy.motion.line_end",
      "end": "vy.motion.line_end",
      "g g": "vy.motion.document_start",
      "g j": "vy.motion.display_down",
      "g down": "vy.motion.display_down",
      "g k": "vy.motion.display_up",
      "g up": "vy.motion.display_up",
      "G": "vy.motion.document_end",
      "x": "vy.edit.delete",
      "d": "vy.operator.delete",
      "y": "vy.operator.yank",
      "Y": "vy.edit.yank_line",
      "p": "vy.edit.paste_after",
      "P": "vy.edit.paste_before",
      ".": "vy.edit.repeat",
      "I": "vy.mode.insert_line_start",
      "A": "vy.mode.insert_line_end",
      "v": "vy.mode.select",
      "V": "vy.mode.select_line",
      "ctrl-v": "vy.mode.select_block",
      "u": "vy.history.undo",
      "ctrl-r": "vy.history.redo",
      "ctrl-d": "vy.view.scroll_half_page_down",
      "ctrl-u": "vy.view.scroll_half_page_up",
      "z z": "vy.view.center_cursor",
      "z t": "vy.view.scroll_cursor_top",
      "z b": "vy.view.scroll_cursor_bottom",
      "z h": "vy.view.scroll_right",
      "z l": "vy.view.scroll_left",
    },
  },

  // 4b. エディタ挿入遷移（オペレータ待機時は operator.inner/around と排他）
  {
    "when": "editorFocus && !insertMode && !operatorPending && !textObjectPending",
    "bindings": {
      "i": "vy.mode.insert",
      "a": "vy.mode.insert_after",
    },
  },

  // 5. エディタ・ノーマル専用
  {
    "when": "editorFocus && normalMode && !operatorPending && !textObjectPending",
    "bindings": {
      "o": "vy.mode.open_below",
      "O": "vy.mode.open_above",
      "g v": "vy.edit.last_visual",
    },
  },

  // 6. エディタ・インサート専用
  {
    "when": "editorFocus && insertMode",
    "bindings": {
      "enter": "vy.insert.newline",
      "tab": "vy.insert.tab",
      "backspace": "vy.insert.backspace",
      "left": "vy.insert.left",
      "right": "vy.insert.right",
      "up": "vy.insert.up",
      "down": "vy.insert.down",
      "home": "vy.insert.home",
      "end": "vy.insert.end",
    },
  },

  // 7. エディタ・オペレータ待機
  {
    "when": "editorFocus && operatorPending",
    "bindings": {
      "i": "vy.operator.inner",
      "a": "vy.operator.around",
    },
  },

  // 8. エディタ・テキストオブジェクト待機
  {
    "when": "editorFocus && textObjectPending",
    "bindings": {
      "w": "vy.text_object.word",
      "p": "vy.text_object.paragraph",
      "(": "vy.text_object.paren",
      ")": "vy.text_object.paren",
      "[": "vy.text_object.bracket",
      "]": "vy.text_object.bracket",
      "{": "vy.text_object.brace",
      "}": "vy.text_object.brace",
      "<": "vy.text_object.angle",
      ">": "vy.text_object.angle",
      "\"": "vy.text_object.double_quote",
      "'": "vy.text_object.single_quote",
      "`": "vy.text_object.backtick",
    },
  },

  // 9. UI / サーフェス共通カーソル移動（一覧、ツリー、表、ボタン、未フォーカスUI）
  {
    "when": "hasSelection || uiRole == 'button' || (paneKind == 'ui' && !uiRole)",
    "bindings": {
      "j": "vy.ui.next",
      "down": "vy.ui.next",
      "ctrl-n": "vy.ui.next",
      "k": "vy.ui.prev",
      "up": "vy.ui.prev",
      "ctrl-p": "vy.ui.prev",
      "g g": "vy.ui.first",
      "G": "vy.ui.last",
      "enter": "vy.ui.activate",
    },
  },

  // 10. UI / サーフェス共通ページ送り・選択（選択可能項目を持つもの）
  {
    "when": "hasSelection",
    "bindings": {
      "ctrl-d": "vy.ui.page_down",
      "ctrl-u": "vy.ui.page_up",
      "x": "vy.ui.toggle",
      "y": "vy.ui.yank",
    },
  },

  // 11. ツリー固有操作
  {
    "when": "uiRole == 'tree'",
    "bindings": {
      "l": "vy.ui.expand",
      "h": "vy.ui.collapse",
    },
  },

  // 12. UI ペイン内フォーカス移動
  {
    "when": "paneKind == 'ui' && !modal",
    "bindings": {
      "tab": "vy.ui.focus_next",
      "backtab": "vy.ui.focus_prev",
    },
  },

  // 13. モーダルサーフェス共通操作
  {
    "when": "modal",
    "bindings": {
      "esc": "vy.ui.cancel",
      "backspace": "vy.ui.delete_char",
      "ctrl-u": "vy.ui.clear_input",
    },
  },

  // 14. Ex コマンドライン固有操作
  {
    "when": "overlay == 'ex'",
    "bindings": {
      "up": "vy.ui.history_prev",
      "ctrl-p": "vy.ui.history_prev",
      "down": "vy.ui.history_next",
      "ctrl-n": "vy.ui.history_next",
      "tab": "vy.ui.complete_next",
      "backtab": "vy.ui.complete_prev",
      "enter": "vy.ui.activate",
    },
  },
]
```

### 4. 規則

1. **`when` はブロックに 1 回だけ書く**。省略されたブロックは無条件（グローバル）に適用されます。
2. **間接参照を作らない**。プロファイル定義やスコープ継承（`extends`）は導入せず、条件とキーの対応関係をその場ですべて読み取れるようにします。
3. **キー衝突と順序依存の完全な排除**:
   - `operatorPending` や `textObjectPending` の間は基本移動キー（`i` や `w` など）と条件レベルで排他（`!operatorPending && !textObjectPending`）にします。
   - モーダルサーフェス内では、`ctrl-u` は `vy.ui.clear_input` のみとなり、以前のデッドコード（`vy.ui.page_up` の無効な上書き）を解消します。
   - ブロックの並び順を入れ替えてもキー解決結果が変わりません。
4. **互換性の維持**:
   - `init.ts` やプラグインからの `keymap.bind({ keys, command, when })` API は変更しません。
   - ブロック形式は `builtin/keymaps/default.json` のロード時に展開されて同一の `Binding` 構造体になります。

---

## 計測と構造比較

| 指標                                 | 現行（平坦配列） | 旧 ADR34 案（プロファイル＋継承） | 本 ADR 決定（直交化＋単層ブロック） |
| :----------------------------------- | :--------------- | :-------------------------------- | :---------------------------------- |
| ファイル行数                         | 736              | 約 190                            | **約 210**                          |
| スキーマの階層                       | 1 層（平坦）     | 2 層（profiles + scopes）         | **1 層（ContextBlock[]）**          |
| 参照ホップ数                         | 0                | 2（scope → profile / extends）    | **0（直読）**                       |
| 展開後のバインディング総数           | 147              | 127                               | **112（完全な重複ゼロ）**           |
| 定義されたキーブロック（スコープ）数 | 17 条件          | 16 スコープ + 6 プロファイル      | **14 ブロック**                     |
| 同一キー表の複製                     | 1 組（13 キー）  | 0（プロファイル共有）             | **0（直交条件による集約）**         |
| 最長条件の出現回数                   | 44 回            | 1 回                              | **1 回**                            |
| 暗黙の順序依存（シャドウイング）     | あり             | あり（配列後勝ちを仕様化）        | **なし（条件レベルで完全排他）**    |
| 5 役割の否定列挙                     | 11 回            | 集約（`noRole` 追加）             | **0 回（既存文法の `!uiRole`）**    |

---

## 帰結

### 良い面

- **認知負荷の最小化**: プロファイル名を探したり、親スコープの `extends` を遡る必要が一切ありません。ブロックを読めば「どの条件でどのキーが効くか」が 1 行で分かります。
- **直交したフォーカスモデル**: 背面ペインのプロパティがオーバーレイに漏れなくなり、`overlay == 'none'` をあちこちに防衛的に書く必要がなくなります。
- **クリーンなブール式**: 5 役割の否定列挙やオーバーレイ名の列挙が消え、`editorFocus`、`modal`、`hasSelection` などの直感的な語彙で記述できます。
- **完全な順序非依存**: 条件がすべて直交または排他になるため、ファイルの行やブロックを入れ替えても挙動が変わりません。Git の差分レビューが極めて安全になります。
- **展開コードの簡潔さ**: ローダー側で行うのは「ブロックの `when` を各キーに配ってフラットな `Binding` にする」ことだけであり、プロファイルの展開、循環 `extends` の検出、継承の上書きマージなどの複雑なロジックが不要です。

### 悪い面・課題

- 単純な平坦配列からブロック構造になるため、ローダー（`crates/vy-app/src/keymap.rs`）のパース処理の差し替えが必要です。
- 展開後のバインディング数が 147 から 112 に減少するため、バインディング総数を固定値でアサートしている既存テスト（`unbind` 件数など）の期待値を更新する必要があります（解決されるコマンド結果自体は同一です）。

### 見直す条件

- ユーザーキーマップ（`~/.config/vy/keymap.json`）の仕様を策定する際。本 ADR のブロック形式はそのままユーザー設定形式としても自然に拡張可能です。

---

## 実装計画

1. **コンテキスト生成の直交化**:
   - `crates/vy-app/src/view/mod.rs` の `apply_key_context` を修正し、モーダルサーフェスが開いている際は背面ペインの `paneKind` をマスクし、サーフェス自身の `uiRole` を設定するようにする。
   - `KeyContext` に `modal` および `hasSelection` 述語を追加する。
2. **ローダーの改修**:
   - `crates/vy-app/src/keymap.rs` に `ContextBlockSpec`（`when: Option<String>`, `bindings: HashMap<String, String>`）を定義し、ロード時に平坦な `Binding` へ展開する。
3. **キーマップ移行とデッドコード整理**:
   - `builtin/keymaps/default.json` を新ブロック形式に置き換える。
   - モーダルサーフェスでの `ctrl-u` の重複定義を解消する。
4. **検証テストの拡充**:
   - 到達しうる全コンテキスト（160 パターン）において、キー解決結果が現行と完全に一致することをテストで保証する。
   - `cargo test --workspace`、`cargo clippy --workspace --all-targets`、`cargo fmt --all --check`、`pnpm format:check` を通す。
