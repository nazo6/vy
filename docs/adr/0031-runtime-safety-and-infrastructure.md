---
id: "0031"
title: "実行時・基盤・セキュリティの負債整理計画"
type: "adr"
status: "accepted"
category: "maintenance"
date: 2026-10-25
related_adrs: ["0030"]
tags: ["security", "data-integrity", "capabilities", "ci", "performance"]
---

# ADR 0031: 実行時・基盤・セキュリティの負債整理計画

- **状態**: 採用 (accepted)。Phase 0 と Phase 5 の一部（ホイールのルーティング）が完了、残りは未着手
- **日付**: 2026-10-25
- **カテゴリー**: maintenance
- **関連タスク**: 品質改善、Phase 4 の仕上げ、Phase 5 以降の前提整備
- **進捗**: Phase 0 は実施済みです（[worklog](../worklogs/2026-10-runtime-safety-infrastructure.md)）。Phase 5 のホイールのルーティングは先行して実装済みです（[worklog](../worklogs/2026-10-30-mouse-pane-and-cursor.md)）。残りは Phase 1–4 と Phase 5–6 の未完了項目です。
- **目的**: 純粋ロジックの整理（[`0030-logic-state-refactor.md`](./0030-logic-state-refactor.md)）とは独立に、実行時の安全性・データ保護・開発基盤に残っている負債を洗い出し、優先度付きで解消する。

## 1. 背景と範囲

純粋ロジック側の ADR（[`0030-logic-state-refactor.md`](./0030-logic-state-refactor.md)、以下「0030」）は「状態の持ち方」と「視覚行計算」を対象にした。本 ADR はそれ以外、つまり**利用者のデータを守る経路・プラグインとの境界・ビルドと検証の基盤・モジュール構造**を対象にする。文書構造の変更で旧来の改善バックログ表は解体されたため、非ロジック系の残債は本ドキュメントで追跡する。

先行して作成した改善計画メモの項目を実コードで検証したところ、次の2点が判明した。

- `pane/resize` のクライアント連携は**実装済み**（`3922cd8`）。キーバインド、Ex、ディバイダーのドラッグまで揃っており、項目として残す必要はない。
- 「codegen の差分チェックは CI のみ」という前提は**事実と異なる**。リポジトリに `.github/` がなく、CI は存在しない（`docs/development.md` には CI 前提の記述が残っている）。

以下では、実装済み・前提違いの項目を除外したうえで、新たに確認した負債を優先度順に並べる。

## 2. 既存計画の検証結果

| 項目                                   | 判定                         | 根拠                                                                                                                                                                                                               |
| :------------------------------------- | :--------------------------- | :----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ウィンドウ終了時の未保存保護           | 妥当（未実装）               | `crates/vy-app/src/ui/titlebar.rs` の閉じるボタンが `window.remove_window()`。OS 起点のクローズに使う `on_window_should_close` は未登録                                                                            |
| `tabstop` 設定の導入                   | 妥当（未実装）               | `DEFAULT_TAB_STOP` が描画・モーション・ホイール・IME の各所に固定値で入っている。`crates/vy-client-api/src/settings.rs` に `editor.tabStop` がない                                                                 |
| ホイールのホバーペイン対応             | 妥当（未実装）               | `crates/vy-app/src/view/mod.rs` の `on_scroll_wheel` が `active_pane` 固定                                                                                                                                         |
| `shim.ts` と `bindings.ts` の型重複    | 妥当（ただし単純置換は不可） | `crates/vy-js/src/shim.ts` の手書き型に対し、生成側の `CommandArgInfo` は `choices` / `required` / `rest` が非オプショナル。`CommandsRegisterParams` を使い、`Partial` などで受ける必要がある                      |
| Specta の二重ヘッダー                  | 妥当                         | `crates/vy-api-schema/src/codegen.rs` の `.header(...)` と Specta 既定のヘッダーが重複し、`packages/vy-api/types/bindings.ts` の先頭に2行出ている                                                                  |
| codegen 差分チェックを `cargo test` へ | 方向は妥当、根拠は要修正     | `vy-api-codegen --check` は実装済みで現在は clean。ただし「CI のみ」ではなく CI 自体がない。また `--check` は `packages/vy-api/types` と `api-base.d.ts` だけを比較し、`shim.d.ts` と `generated/shim.js` は対象外 |
| 折り返しキャッシュの健全化             | 妥当（主因は別）             | 64bit ハッシュ衝突の実害はほぼ無い。実際のコストは「可視行ごとのハッシュ計算と `String` 確保」「1024 件で全消去」。視覚行の単一実装（0030 Phase A）と同時に扱うのが効率的                                          |
| `ViewState` と `Editor` の手動同期     | 妥当                         | 呼び出しは 30 箇所超。ただし RAII ガードは `documents` と `panes` の二重可変借用で成立しない。0030 Phase F の `PaneViewport` で扱う                                                                                |
| `commands.rs` の分割                   | 妥当（対象を追加）           | `crates/vy-app/src/view/commands.rs` は約 1772 行。同様に `crates/vy-app/src/commands.rs`（約 1635 行、Ex パーサ・レジストリ・引数検証）も対象                                                                     |
| `pane/resize` のクライアント連携       | 実装済み（除外）             | `PaneCommand::Resize` / `Equalize`、`Ctrl-w + - > < =`、`:resize` / `:equalize`、ディバイダーのドラッグまで実装されている                                                                                          |
| レジスタ（ヤンク・削除）の共有         | 対象外                       | 状態モデルの変更なので 0030 側の課題として扱う。`.` の履歴は Vim と同じく文書単位のままでよい                                                                                                                      |

## 3. 新たに確認した負債

### 3.1 優先度 P0: サンドボックスの境界（`..` とシンボリックリンク）

- **症状**: `${workspace}/**` を承認したプラグインが、`file:///workspace/../etc/passwd` のような URI で作業ディレクトリ外を読める。workspace 内のシンボリックリンク経由も同様。
- **根拠**:
  - `crates/vy-capabilities/src/lib.rs` の `glob_match` は文字列一致のみで、`..` を畳まない。
  - `crates/vy-app/src/api/mod.rs` の `check_capability` は `file:` URI をパス文字列へ変換して照合するだけ。
  - `crates/vy-server/src/session/fs.rs` の `uri_to_path` も正規化しない。`std::fs::read` は OS が `..` を解決する。
- **影響**: 承認済みプラグインの権限が、実質「ホーム全体の読み書き」に広がる。マニフェストの宣言と実際の権限が一致しない。
- **修正方針**:
  1. 照合前にパスを正規化し、`..` を畳む。パターン側も同じ規則で正規化する。
  2. 実行直前に実体パスを `canonicalize` し、許可ルート配下かを判定する（glob は宣言の展開だけに使う）。
  3. サーバー側でも同じ検査を行う。クライアントを信頼して片側だけに置かない。
- **テスト**: `..`、`%2e%2e`、末尾 `..`、シンボリックリンク、存在しないパス、ルート外への書き込み。

### 3.2 優先度 P0: 保存が原子的でない・外部変更を検知しない

- **症状**: 書き込み中にクラッシュすると、ファイルが切り詰められた状態で残る。外部で変更されたファイルを、警告なしに上書きする。
- **根拠**: `crates/vy-server/src/session/buffer.rs` の `save_document` が `std::fs::write`（truncate → write）。`crates/vy-server/src/session/fs.rs` の `fs_write_file` も同じ。mtime の監視は設定ファイルにしかない。
- **影響**: 保存処理そのものがデータ損失の原因になりうる。エディタとして看過できない。
- **修正方針**:
  1. 同一ディレクトリに一時ファイルを書き、`sync_all` の後に `rename` する共通ヘルパーを作る。元のパーミッションを引き継ぐ。失敗時は一時ファイルを削除する。
  2. `document.save` と `fs.writeFile` の両方をこのヘルパーに寄せる。
  3. 保存前に mtime / size を記録し、変化していればエラーにして `:w!` での強制保存を要求する。

### 3.3 優先度 P0: 終了経路に未保存の保護がない

- **症状**: タイトルバーの×、または OS のウィンドウを閉じる操作で、未保存の変更がそのまま失われる。
- **根拠**:
  - `crates/vy-app/src/ui/titlebar.rs` は `window.remove_window()` を呼ぶ。gpui-pre 0.3.9 の `Window::remove_window` は `removed` フラグを立てるだけで、`on_window_should_close` を経由しない。
  - OS 起点のクローズは `platform_window.on_should_close` に流れるが、`EditorView` は登録していない。
  - `crates/vy-core/src/layout.rs` の `Layout::close` は最後のペインを閉じられない（呼び出し側がウィンドウを閉じる想定）。しかしクライアントに終了コマンドがないため、`:q` で終了できない。
- **修正方針**:
  1. `EditorView::request_quit` を1つ作り、タイトルバーの×と `window.on_window_should_close` の両方から呼ぶ。
  2. 未保存の文書があればモーダルを出し、「保存」「破棄」「キャンセル」を選ばせる。破棄するまでウィンドウを閉じない。
  3. `:qa` / `:qa!` / `:wq` / `:wqa` を追加し、`PaneCommand::Close` が最後のペインだった場合は「ウィンドウを閉じる」にフォールバックする。
- **注意**: 子プロセスは `Client` の `Drop` で kill される。レイアウトはペイン操作のたびに保存済みだが、未保存の本文はクライアントのメモリにしかない（[`../specs/architecture.md`](../specs/architecture.md)）。破棄確認が唯一の防波堤になる。

### 3.4 優先度 P1: フレーム上限で接続が落ちる・編集が無言で消える

- **症状**: 64 MiB を超える応答（大きな文書の `buffer/snapshot` など）でサーバーが終了する。クライアント側で符号化に失敗した要求は送られず、本文だけが進む。
- **根拠**:
  - `crates/vy-proto/src/frame.rs` の `MAX_FRAME_LEN` が 64 MiB。
  - `crates/vy-server/src/stdio.rs` の `write_message` がエラーを返し、`serve_with` が終了、`main` が失敗コードで終わる。
  - `crates/vy-app/src/rpc.rs` の `write_loop` は符号化失敗時に `continue` で要求を捨てる。
- **修正方針**:
  1. サーバーは大きすぎる応答を per-request のエラーに変換し、接続を維持する。
  2. スナップショットは分割転送（[`../specs/protocol.md`](../specs/protocol.md) の Blob / Stream）にするか、上限超過を明示的なエラーとして返す。
  3. クライアントは符号化に失敗した要求を呼び出し元へ返し、バージョンを進めない。
  4. 文書を開く時点でサイズを確認し、上限を超える場合は理由を出す。

### 3.5 優先度 P1: `process:exec:<cmd>` を承認すると kill / stdin が拒否される

- **症状**: `process:exec:cargo` だけを承認したプラグインが、自分が起動したプロセスに `writeStdin` も `kill` もできない。
- **根拠**: `crates/vy-app/src/api/host/process.rs` の `kill` / `write_stdin` が `process:exec`（target なし）を要求する。target 付きの宣言は target なしの要求をカバーしない。
- **修正方針**: 起動した PID の所有関係を記録し、所有するプロセスへの操作は許可する。または `process:control` のような別種別に分ける。

### 3.6 優先度 P1: 終了系コマンドがなく、文書の記述も矛盾している

- **症状**: `:wq` がない。最後のペインでは `:q` も終了にならない。
- **根拠**: `crates/vy-app/src/commands.rs` に `write_quit` 系がない。[`../specs/commands.md`](../specs/commands.md) は `:wq` を未実装としつつ、別の箇所で「`!` 付きの強制終了（`:q!`）は初期版では扱わない」と書いている。実際には `bang` が `force` として渡り、`:q!` は動作する。
- **修正方針**: 3.3 と合わせて実装し、文書の矛盾を解消する。

### 3.7 優先度 P1: CI がなく、codegen の検証範囲も狭い

- **症状**: `cargo test` / `clippy` / `fmt` / codegen 差分が自動で検証されない。`shim.js` と `shim.d.ts` の生成忘れも検出できない。
- **根拠**: `.github/` が存在しない。[`../development.md`](../development.md) には「CI では `git diff --exit-code`」とある。`crates/vy-api-codegen/src/main.rs` の `check_client` は `types/` と `api/` のみ比較し、`scripts/generate-shim.mjs` の出力は対象外。
- **修正方針**: CI を追加し、`fmt` / `clippy` / `test` / `check:types` / `codegen --check` / `generate:api` 後の差分確認を回す。`--check` に `shim.d.ts` と `generated/shim.js` を含める。あわせて `cargo test` から呼べる統合テストにする。

### 3.8 優先度 P1: 設定・表示の残債

- **`tabstop` 設定**: `DEFAULT_TAB_STOP` が `text_layout.rs`、`editor/motion.rs`、`view/render.rs`、`view/commands.rs`、`ui/pane.rs`、`on_scroll_wheel` に固定値で入っている。`crates/vy-client-api/src/settings.rs` に `editor.tabStop` を追加し、`PaneState::is_wrap` と同じ形で解決する。0030 の `VisualOptions` に `tab_stop` を渡す形にしておくと移行が楽になる。
- **ホイールのルーティング**: `on_scroll_wheel` は `active_pane` 固定。オーバーレイ表示中も背後のエディタが動き、`ui` / `list` ビューは無反応。`geometry::collect_pane_rects` と `window.bounds()` でポインタ下のペインを求め、オーバーレイが開いている間はスクロールを止める。
- **フレーム毎コスト**: `PaneState::is_wrap` が毎回 `Value` を clone し、`Editor::line` が可視行ごとに `String` を確保し、`ui/pane.rs` の文字幅キャッシュが無上限。0030 Phase A と同時に整理する。

### 3.9 優先度 P2: 構造と整合性

- **コマンド ID の定義が2系統**: `crates/vy-client-api/src/command.rs` は `define_command_enum!` と `strum(serialize = ...)` が混在している。`builtin/keymaps/default.json` の ID が解決可能かを検証するテストもない。
- **レイアウト幾何の二重実装**: `crates/vy-app/src/view/render.rs` のピクセル計算と `crates/vy-app/src/geometry.rs` の比率計算が別実装。ホイールやクリックのヒットテストを入れる前に、スクリーン座標の矩形計算を1箇所にまとめる。
- **クライアント本文ミラーの整合性**: 仕様どおりクライアントが本文の写しを持つが、`buffer/did_change` に Ack がない。コアは拒否時だけ `buffer/resync` を返すため、要求が届かなかった場合は永続的にずれる。再接続時以外にもバージョンを突き合わせる保険を検討する。
- **ケイパビリティ検査の分散**: 個別の `check_capability` 呼び出しが `crates/vy-app/src/api/host/*.rs` に並ぶ。マクロの `dispatch` には要求コンテキストがないため、`capability` と `target`（params のどのフィールドか）を宣言するか、`authorize(method, params)` フックを1本足す。
- **モジュールの肥大化**: `crates/vy-app/src/view/commands.rs`（約 1772 行）、`crates/vy-app/src/commands.rs`（約 1635 行）、`crates/vy-app/src/view/rpc.rs`（約 766 行）、`crates/vy-app/src/view/plugin.rs`（約 698 行）。
- **`:resize` の引数宣言がない**: `PaneCommand::Resize` は `ArgSpec::None` のため、Ex と `commands.execute` から `value` を渡せない。`run_pane_resize_command` が持つ `+N` / `-N` / パーセント / 比率の解釈は現状到達しない（Phase 0 で確認）。
- **固定値と未使用の宣言**: `workspace.info` の `trusted` が常に `true`（`crates/vy-app/src/api/host/workspace.rs`、`crates/vy-app/src/plugin_host.rs`）。`clipboard:*` / `secrets` / `net` / `openExternal` / `fs:local:*` / `process:local` は定義だけで検査箇所がない。
- **状態ファイルの書き込み**: `crates/vy-server/src/session/state.rs` の `persist_session` も非原子的。レイアウトのみなので影響は小さいが、3.2 のヘルパーに寄せられる。

## 4. フェーズ計画

各フェーズは独立してコミットできる単位とする。P0 は「利用者のデータと境界」に直結するため先に置く。

### Phase 0: 文書と実装の突き合わせ

- [x] [`../specs/commands.md`](../specs/commands.md) の `:q!` の記述矛盾を解消する（実装済みとして扱う）。
- [x] `pane/resize` が実装済みであることを前提に、古い計画由来の記述が残っていないか確認する。
- [x] 実装済みなのに未実装と書かれている項目、その逆を一覧化して直す。
- [x] `:wq` / `:qa` の予定を Phase 1 の完了条件として本文に反映する。

確認結果（2026-10-26）:

- `:q!` は `PaneCommand::Close` の `bang` として実装済み。未保存の確認は飛ぶが、
  ウィンドウ最後の 1 枚は `Layout::close` の `LastPane` で閉じられない。
  `commands.md` と `panes.md` の記述をこの挙動に合わせた。
- `:resize` / `:res` と `:equalize` は登録済みだが `commands.md` の表になかった
  ため追加した。`PaneCommand::Resize` の `ArgSpec` が `None` のため、
  `run_pane_resize_command` が解釈する `+N` / `-N` / パーセント / 比率は Ex から
  渡せない。現状は引数なし（高さ +5%）のみ動く。引数宣言は Phase 6 に送る。
- `pane/resize` のキーボード・Ex・ディバイダーの実装は仕様書の記述と一致して
  おり、修正は不要だった。
- `keymap.md` の「認識するキー」に `Delete`、`Shift-Tab`、`Ctrl` + 英字全般が
  抜けていた。Insert の `Delete` はキー名を解釈済みで、未実装の理由は「処理が
  ない」ため備考を直した。`editor.rs` という古いパスも `editor/` に直した。
- `:wq` / `:qa` は未実装で、記述は正しかった。

完了条件: `docs/` 内で `:q!` / `:wq` / `pane/resize` の記述がコードと一致する。

### Phase 1: データ保護と終了フロー

- [ ] 原子的な保存ヘルパー（temp → `sync_all` → `rename`、パーミッション継承、失敗時の後始末）を作る。
- [ ] `document.save` と `fs.write_file` をこのヘルパーに置き換える。
- [ ] 保存前の mtime / size 比較と、`:w!` による強制保存を実装する。
- [ ] `EditorView::request_quit` を追加し、タイトルバーの×と `window.on_window_should_close` の両方から呼ぶ。
- [ ] 未保存時の確認モーダル（保存 / 破棄 / キャンセル）を実装する。
- [ ] `:qa` / `:qa!` / `:wq` / `:wqa` を追加し、最後のペインの `:q` を終了にフォールバックさせる。
- [ ] テスト: 保存の成功・失敗・強制保存、未保存状態での終了要求、最後のペインの `:q`。

完了条件: どの経路でウィンドウを閉じても、未保存の本文が無断で消えない。
`:wq` / `:qa` / `:qa!` / `:wqa` で終了でき、最後のペインの `:q` も終了になる。

### Phase 2: サンドボックスの境界

- [ ] ケイパビリティ照合の前処理としてパス正規化（`..` の畳み込み）を入れる。
- [ ] 実行直前の `canonicalize` と許可ルート配下の判定をサーバー側に追加する。
- [ ] glob は宣言の展開に限定し、実パスの判定には使わない。
- [ ] テスト: `..`、パーセントエンコード、シンボリックリンク、ルート外、書き込み系 API。

完了条件: `${workspace}/**` を承認したプラグインが workspace 外へ到達できないことをテストで示せる。

### Phase 3: 開発基盤（CI と codegen）

- [ ] CI ワークフローを追加する（`fmt` / `clippy` / `test --workspace` / `check:types` / `codegen --check` / `generate:api` 後の差分）。
- [ ] `vy-api-codegen --check` の比較対象に `packages/vy-api/shim.d.ts` と `crates/vy-js/src/generated/shim.js` を加える。
- [ ] codegen チェックを `cargo test` から呼べる統合テストにする。
- [ ] [`../development.md`](../development.md) の CI 記述を実態に合わせる。

完了条件: 生成忘れと整形漏れがローカルとリモートの両方で検出される。

### Phase 4: プロトコル耐性

- [ ] 巨大な応答を per-request のエラーに変換し、接続を落とさない。
- [ ] クライアントの符号化失敗を要求元へ返し、状態を進めない。
- [ ] 文書サイズの上限を open 時に検査し、理由を表示する。
- [ ] スナップショットの分割転送の要否を決める（§6）。
- [ ] バージョン不一致を検出する保険（定期またはイベント時の突き合わせ）を入れる。
- [ ] テスト: 上限超過の要求・応答、無言ドロップが起きないこと。

完了条件: 単一の巨大な文書やファイルでセッションが落ちない。

### Phase 5: プロセスの権限と設定・表示

- [ ] 所有 PID に基づく `kill` / `write_stdin` の許可を実装する。
- [ ] `editor.tabStop` を設定として追加し、描画・モーション・ホイール・IME の固定値を置き換える。
- [x] ホイールをポインタ下のペインに届け、オーバーレイ表示中は背後をスクロールしない（2026-10-30）。
- [ ] `is_wrap` の clone、`Editor::line` の確保、文字幅キャッシュの上限を整理する。
- [ ] テスト: `process:exec:<cmd>` 承認下の stdin / kill、`tabstop` の反映、ホイールの対象ペイン。

完了条件: 承認した権限の範囲でプロセスを操作でき、表示設定が即座に反映される。

### Phase 6: 構造の整理

- [ ] コマンド ID の定義方式を1つに統一し、既定キーマップの ID 解決テストを追加する。
- [ ] スクリーン座標の矩形計算を `geometry.rs` に一本化し、描画とヒットテストで共有する。
- [ ] ケイパビリティ検査をマクロ宣言＋共通インターセプタに寄せる。
- [ ] `view/commands.rs` を「ディスパッチ / Ex / パレット / 設定」に分割する。`commands.rs` の Ex パーサと引数検証も分ける。
- [ ] `workspace.info.trusted` と未使用のケイパビリティ種別を、実装するか仕様から外すか決める。
- [ ] `persist_session` を原子的な保存ヘルパーに寄せる。
- [ ] `PaneCommand::Resize` に `value` の `ArgSpec` を宣言し、`:resize +3` / `:resize 50` / `:resize 0.3` を動かす（§3.9）。

完了条件: 同じ役割の実装が1箇所にまとまり、追加時に触るファイルが減っている。

## 5. 検証

### 自動テスト

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
pnpm check:types
cargo run -p vy-api-codegen -- --check
git diff --exit-code packages crates/vy-js/src/generated
```

新規テスト:

- ケイパビリティ: `..`、パーセントエンコード、シンボリックリンク、ルート外
- 保存: 成功、書き込み失敗、強制保存、外部変更の検出
- 終了: 未保存ありの×、`on_window_should_close`、`:qa!`、最後のペインの `:q`
- フレーム: 上限超過の要求と応答、符号化失敗
- プロセス: `process:exec:<cmd>` 承認下の stdin / kill
- codegen: `--check` が `shim.js` / `shim.d.ts` の差分を検出する

### 手動確認

1. 未保存のまま×を押し、確認モーダルが出ること。保存・破棄・キャンセルの3経路。
2. 未保存のまま Alt+F4（OS 起点）でも同じ確認が出ること。
3. workspace 外を指す `fs.readFile` が、承認済みプラグインから拒否されること。
4. 大きなファイルを開いてもセッションが切れないこと。
5. `:wq` と `:qa!` で終了できること。
6. 外部でファイルを書き換えてから `:w` し、警告が出ること。`:w!` で保存できること。

### 規約確認

```sh
rg -n '[ぁ-んァ-ヶ一-龠]' crates
```

## 6. 判断が必要な点

- **パス正規化の方式**: 文字列の正規化だけで足りるか、`canonicalize` を必須にするか。存在しないパス（新規作成）の扱いを決める必要がある。
- **保存の強制上書き**: 外部変更を検出したとき、既定をエラーにするか警告にするか。`:w!` の UX をどこに出すか。
- **終了確認の形**: モーダル3択か、`:qa` のエラー表示＋再実行か。複数文書のときの一括保存の扱い。
- **フレーム上限**: 分割転送を今やるか、上限超過を明示エラーにして後回しにするか。
- **CI の範囲**: GitHub Actions を前提にしてよいか。macOS / Windows のビルドを含めるか。
- **`tabstop` のスコープ**: `Window` か `Global` か。プロジェクト設定で上書きを許すか。
- **ミラー整合性の検証**: 再接続時のみか、一定間隔か、イベント時か。

## 7. 対象外

- 状態機械・視覚行計算の整理（[`0030-logic-state-refactor.md`](./0030-logic-state-refactor.md)）
- レジスタのワークスペース共有、未実装の編集キー
- フローティングペイン、`session.json` によるウィンドウ位置の復元（保留中）
- Phase 6 以降（バックエンドプラグイン、LSP、Webview）

## 8. 関連ドキュメント

- [`../specs/architecture.md`](../specs/architecture.md): プロセスモデル、状態の所在、再接続
- [`../specs/protocol.md`](../specs/protocol.md): フレーム、ストリーム、Blob、エラー
- [`../specs/api.md`](../specs/api.md): ケイパビリティ、`fs.*` / `process.*`
- [`../specs/plugin-system.md`](../specs/plugin-system.md): マニフェスト、`trusted.json`、承認
- [`../specs/commands.md`](../specs/commands.md): Ex コマンドの一覧と実装状況
- [`../development.md`](../development.md): 検証手順と CI の記述
- [`./0030-logic-state-refactor.md`](./0030-logic-state-refactor.md): 純粋ロジック側の ADR
