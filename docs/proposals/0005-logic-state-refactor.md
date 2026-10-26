---
id: "0005"
title: "純粋ロジックの状態モデル整理計画"
type: "proposal"
status: "open"
date: 2026-10-25
tags: ["refactoring", "state-machine", "editor", "keymap", "text-layout"]
---

# [Proposal] 純粋ロジックの状態モデル整理計画

- **日付**: 2026-10-25
- **関連タスク**: 品質改善（保守性 P2）、`command-surfaces` / `pane/resize` の後始末
- **状態**: open（未着手）
- **目的**: `vy-app` の GPUI
  非依存ロジックに散在する「暗黙のフラグ集合」と「視覚行計算の重複実装」を、明示的な状態型と単一実装へ置き換える。挙動は変えず、今後の機能追加（マウス操作、`tabstop`
  設定、折り返し改善、LSP 座標変換）で触る箇所を減らす。

## 1. 背景

直近の機能追加で、次の3種類の「ごちゃつき」が育っている。

1. **暗黙の状態**: `Option<T>` と `bool`
   の組み合わせで表現された状態機械が、フィールドとして散在している。不正な組み合わせが作れてしまい、`clear_*()`
   の呼び忘れがバグになる。
2. **重複した計算**: 「行 → 折り返しチャンク → 視覚行 →
   表示桁」の変換が複数モジュールに再実装されている。クランプ条件が場所ごとに微妙に異なる。
3. **定型のコピペ**: 7 variant のモーダルや 6
   種類のコマンド列挙型に対して、同じ形の `match` / `if let`
   連鎖が繰り返されている。

いずれも「純粋ロジック」の層で完結するため、GPUI
を動かさずにテストで固定しながら直せる。

## 2. 現状分析

### 2.1 視覚行マッピングの 6 重実装

| 場所                                                                                  | 実装している変換                                               |
| :------------------------------------------------------------------------------------ | :------------------------------------------------------------- |
| `crates/vy-app/src/editor/motion.rs` の `target_display_vertical`                     | カーソル位置 → チャンク、チャンク単位の上下移動、表示桁 → 位置 |
| `crates/vy-app/src/view/mod.rs` の `on_scroll_wheel`（折り返し分岐）                  | 行・チャンクを跨ぐ行数の加減算                                 |
| `crates/vy-app/src/view/render.rs`（カーソル行、ホバー、可視化）                      | 位置 → 視覚行、表示桁                                          |
| `crates/vy-app/src/view/input.rs` の `bounds_for_range` / `character_index_for_point` | 位置 ↔ 視覚行（両方向）                                        |
| `crates/vy-app/src/ui/pane.rs` の `prepare_editor_paint`                              | 描画行ごとのチャンク取得                                       |
| `crates/vy-app/src/text_layout.rs` の `chunk_line` / `cached_chunk_line`              | チャンク分割そのもの                                           |

問題点:

- 同じ「折り返し跨ぎの前後移動」が `target_display_vertical` と
  `on_scroll_wheel` に別実装で存在する。
- 正規表現上の `chunk_line` 呼び出しのうち、`cached_chunk_line` が返す
  `Arc<LineChunks>` を活かさず `.chunks.clone()`
  している経路があり、可視行ごとに `Vec` を確保している。
- クランプ（Normal は行末 1 文字手前、Select
  は行末まで）が場所ごとに個別実装されている。

### 2.2 `Editor` の暗黙入力状態

`crates/vy-app/src/editor/mod.rs` の `Editor`
は、次のフィールドが相互依存している。

- `operator` / `operator_count` / `text_object`: 「`d` 待ち」「`di`
  待ち」を表すが、型では表現されていない。
- `active_register`: 現在のコマンドにだけ有効な隠れ状態。早期 return
  のたびに手で `None` に戻している。
- `insert_buffer` / `insert_kind`: Insert セッションの状態。
- `preferred_character` / `preferred_display_col`: 「縦移動で保つ桁」の2系統。

`execute` は冒頭で pending operator / text object
を手続き的に処理し、`clear_operator`
を複数箇所から呼ぶ。`crates/vy-app/src/editor/view.rs` の `ViewState`
は同じ3フィールド（`operator` / `operator_count` /
`text_object`）を複製しており、定義が二重になっている。さらに
`ViewState.selection` は `anchor` と `head`
から導出できる値を保持しており、`view_state()` は再計算するが `set_view_state()`
は再計算しない（`clamp_view_state` だけが直す）という非対称がある。

### 2.3 Keymap のプレフィックス状態

`crates/vy-app/src/keymap.rs` の `Keymap` は `pending` / `count` /
`awaiting_register` / `register` / `last_key`
の5フィールドでプレフィックス入力を表している。

- `is_pending()` は3条件の OR で、「何待ちか」は呼び出し側から分からない。
- `feed` は
  Esc・レジスタ・カウント・シーケンスを1関数内で順序依存に処理しており、条件の追加が難しい。
- `expire` の呼び出し元が `feed` とタイマーの2系統に分かれている。

### 2.4 セッション（接続）状態

`crates/vy-app/src/view/mod.rs` の `EditorView` は `client` /
`connection_generation` / `reconnecting` / `reconnect_requested` / `pending`
で接続状態を持つ。

- `crates/vy-app/src/view/rpc.rs` の `connect` と `try_reconnect` が
  `Initialize` の組み立てをコピペしている。
- `reconnect_requested` は設定ファイル監視の 500ms
  ポーリングが拾う「フラグ渡し」。

### 2.5 プラグイン実行のライフサイクル二重管理

- `crates/vy-app/src/plugin/manager.rs` の `PluginState`（`PendingApproval` /
  `Denied` / `Loaded` / `Active` / `Failed`）
- `crates/vy-app/src/view/mod.rs` の `plugin_runtimes` +
  `PluginRuntime.started` + `generation` + `next_plugin_generation`

`crates/vy-app/src/view/plugin.rs` は 16ms のポーリングで `started` を監視して
pump を起動している。`start_script` と `start_plugin_script` はほぼ同一の実装。

### 2.6 Overlay の定型マッチ

- `crates/vy-app/src/overlay.rs` の `Overlay` は 7 variant に対し `input` /
  `input_mut` / `selected` / `selected_mut` / `prefix` / `push_char` /
  `push_str` / `pop_char` / `clear` を同じ形で列挙している。
- `crates/vy-app/src/view/commands.rs` の `handle_palette_key` / `handle_ex_key`
  / `handle_prompt_key` / `handle_approval_key` / `handle_settings_key` /
  `handle_setting_choice_key` / `handle_setting_prompt_key`
  は、上/下/Backspace/文字入力の処理をコピペしている。

### 2.7 コマンド解決の if 連鎖

- `crates/vy-app/src/view/commands.rs` の `execute_invocation` は
  `UiCommand::parse` → `PaneCommand::parse` → `DocumentCommand::parse` → …
  と6連の `if let` で解決している。順序が暗黙の仕様になっている。
- `drain_command_queue` は `execute_invocation` の `bool`
  戻り値でループを制御しており、「真 =
  スクリプトスレッドで継続」という意味が読み取りにくい。
- `crates/vy-app/src/commands.rs` の `Invocation`
  は6つのコンストラクタが同型のリテラルを繰り返す。`CommandSource` と `Owner`
  の対応も二重管理。

### 2.8 混在した型変換

- `crates/vy-app/src/view/commands.rs` の `run_pane_resize_command` は `_raw` の
  `+3` / `-2` / `50` / `0.3` をネストした `if`/`else` で解釈している。値の解釈が
  UI 層に埋まっている。
- `crates/vy-app/src/view/mod.rs` の `apply_key` は「前後の
  `ViewState`・スクロール位置・バージョン・オーバーレイ有無」を4つ退避して最後に比較しており、再描画条件が1つの式に集約されている。

## 3. 設計方針

1. **同時に成立する組み合わせだけを持つ enum にする**。`Option` の並置や `bool`
   フラグで状態を表さない。
2. **派生値は保持しない**。保持する場合は書き込み箇所を1つに限定し、その不変条件を型かテストで守る。
3. **同じ計算は1実装**。視覚行変換は1つの型に集約し、全呼び出し元をそこへ寄せる。
4. **純粋関数は依存を引数で受け取る**。GPUI の `Window` や `Context`
   を状態型に持ち込まない。
5. **GPUI 依存は配線だけ**。状態型は `vy-app` の GPUI 非依存モジュールに置く。
6. **仕様書は原則更新しない**。挙動・用語が変わる場合のみ `docs/specs/`
   を更新する。

## 4. フェーズ計画

各フェーズは独立してコミットできる単位とする。挙動変更は原則行わず、既存テストを通したうえで新しい純粋ロジックのテストを足す。

### Phase 0: 先行して直す小さな不整合

- [x] `ViewState.selection` の更新を `Editor::view_state()`
      経由に限定する（`set_view_state`
      でも再計算するか、フィールドを削除してアクセサにする）。
  - 派生値のため**フィールドを削除**した。描画直前に
    `Editor::range_for(mode, anchor, head)` で求める（`EditorPaintData.selection`）。
- [x] `chunk_line` の `.chunks.clone()` 経路を洗い出し、`Arc<LineChunks>` を返す
      API に統一する（`ui/pane.rs` の方式に寄せる）。
  - 本番の 10 箇所を `cached_chunk_line` に置換。`chunk_line` 系は本番から削除し、
    テスト用のヘルパーは `text_layout` の `mod tests` に移した。
- [x] `active_register` の解除漏れがないかテストで固定する（早期 return
      を含む経路）。
- [x] `reconnect_requested` のポーリング依存を洗い出し、Phase D
      の置き換え対象として記録する。

完了条件: `cargo test -p vy-app --lib` が通り、`chunk_line` の呼び出し箇所が
`cached_chunk_line` だけになる。

実施記録: [`../worklogs/2026-10-26-logic-state-refactor.md`](../worklogs/2026-10-26-logic-state-refactor.md)
（2026-10-26 完了。`vy-app` の lib テストは 196 → 200 件）。

### Phase A: 視覚行モデル（効果最大）

- [x] `crates/vy-app/src/editor/visual.rs`
      を新設し、`VisualOptions { cols, tab_stop }` と `VisualIndex` を定義する。
  - `rows_in(line)`（行の視覚行数）
  - `row_of(position)`（`VisualRow { line, chunk, col }`）
  - `position_at(row, desired_col)`
  - `step(row, delta)`（折り返しを跨ぐ前後移動）
- [x] 行テキストの取得は `&Text` を受け取る形にし、`vy-app` 内で完結させる。
- [x] `target_display_vertical` の前後走査ループを `VisualIndex::step`
      に置換する。
- [x] `on_scroll_wheel` の折り返し分岐を `VisualIndex` に置換する。
- [x] `view/input.rs` の `bounds_for_range` / `character_index_for_point` を
      `row_of` / `position_at` に置換する。
- [x] `view/render.rs` のカーソル・ホバー行計算を `VisualIndex` に置換する。
- [x] Normal / Select の行末クランプを `VisualIndex` の1実装に統合する。
- [x] テスト: 往復一致（`row_of` →
      `position_at`）、折り返し跨ぎ、全角・タブ、空行、最終行。

完了条件: 「行 → チャンク → 視覚行」の計算が `editor/visual.rs`
以外に存在しない。可視行の描画で `Vec` の新規確保が発生しない。

実施記録: [`../worklogs/2026-10-26-logic-state-refactor.md`](../worklogs/2026-10-26-logic-state-refactor.md)
（2026-10-26 完了。`VisualIndex` のテスト 8 件を追加）。`ui/pane.rs` の
`prepare_editor_paint` / `collect_visual_rows` も同じ型に寄せた。手動確認は未実施。

### Phase B: 入力状態機械（エディタ）

- [x] `crates/vy-app/src/editor/input_state.rs` を新設し、`PendingInput`
      を定義する。
  - `Idle`
  - `Operator { op, count, register }`
  - `TextObject { op, count, register, kind }`
- [x] `Editor` と `ViewState` の `operator` / `operator_count` / `text_object`
      を `PendingInput` に置き換える。
- [x] `execute` の前処理を `PendingInput`
      に集約し、`clear_operator` の呼び出しを削除する。
  - `consume(command)` という 1 関数にはせず、`operator()` / `start_operator` /
    `begin_text_object` / `take_text_object` / `clear` に分けた。キャンセル判定
    （`is_operator_command` / `is_text_object_command`）は `execute` に残している。
- [x] `active_register` を廃止し、register を `PendingInput`
      と適用関数の引数で受け渡す。
- [x] Insert セッションを `InsertSession { kind, buffer }` にまとめる。
- [x] `SelectMode` / `SelectLineMode` / `SelectBlockMode` の3ブロックを
      `toggle_select_mode(mode)` に統合する。
- [x] テスト: `d` → motion、`d` → `i` → object、`dd`、カウントの合成、Esc
      でのキャンセル、`.` の再現。

完了条件: 「演算子待ち」を表す `Option`
が複数フィールドにまたがらない。`clear_*` 系の呼び忘れが構造的に起きない。

実施記録: [`../worklogs/2026-10-26-logic-state-refactor.md`](../worklogs/2026-10-26-logic-state-refactor.md)
（2026-10-26 完了。`PendingInput` 5 件とレジスタ保持 3 件のテストを追加）。
`"adw` / `"add` / `"adiw` で register が unnamed に落ちていた既存の不具合も修正した。

### Phase C: Keymap プレフィックス

- [x] `crates/vy-app/src/keymap.rs` に `Prefix` を導入する。
  - `None`
  - `Count { value }`
  - `Register { count }`
  - `Sequence { keys, count, register }`
- [x] `feed` を「`Prefix` の遷移 +
      バインディング照合」に分解する。Esc・タイムアウトは遷移として表現する。
- [x] `expire` の責務を1つにする（`feed` 内、またはタイマーのみ）。
- [x] `view/mod.rs` の `on_key_down` の早期 return
      群を、`InputTarget { Overlay, InsertIme, InsertKeys, Normal }`
      を返す純関数に集約する。
  - IME の振り分けは既存の `ActiveInput` が担っているため、オーバーレイの
    3 分岐だけを `OverlayTarget { OpenEx, Text, Key }` に集約した。
- [x] テスト: 既存の `keymap.rs` のテストを維持し、`Prefix` の遷移テストを追加。

完了条件: `is_pending()` の3条件 OR
が消える。「今なにを待っているか」が1つの値で分かる。

実施記録: [`../worklogs/2026-10-26-logic-state-refactor.md`](../worklogs/2026-10-26-logic-state-refactor.md)
（2026-10-26 完了。`Prefix` の遷移テスト 5 件を追加）。
`"` + レジスタ名の直後がタイムアウトで消えなかった点も修正した。

### Phase D: セッションとプラグインのライフサイクル

- [ ] `crates/vy-app/src/session.rs` を新設し、`SessionState` を定義する。
  - `Disconnected { attempts }`
  - `Connecting { generation }`
  - `Ready { generation }`
  - `Restoring { generation }`
- [ ] `Initialize`
      リクエストの組み立てを1箇所（`initialize_request`）に集約し、`connect` /
      `try_reconnect` の重複を削除する。
- [ ] `reconnect_requested` を廃止し、`vy.reconnect_server`
      から直接接続を張り直す。
- [ ] `view` 側のプラグイン状態を
      `RuntimeSlot { Spawning, Running { generation }, Stopped }`
      に置き換え、`started` フラグを削除する。
- [ ] 16ms ポーリングの supervisor を削除し、ランタイム生成時に直接 pump
      を開始する。
- [ ] `start_script` と `start_plugin_script` を1関数に統合する。
- [ ] テスト: `SessionState` の遷移（切断 → 再接続 → 復元）を GPUI
      なしで検証する。

完了条件: 接続状態を表すフィールドが `SessionState`
に集約される。プラグインの「起動待ち」がポーリングでなくなっている。

### Phase E: dispatch と定型の圧縮

- [ ] `Overlay` の共通部分を `ListInput { input, selected }`
      に集約し、`overlay.rs` の 9 メソッドを 2 アクセサに縮約する。
- [ ] 7 個の `handle_*_key` を「共通キー（上/下/Backspace/文字）→ variant
      固有（Esc/Enter）」の2段に統一し、`scroll_to_item`
      の呼び出しを1箇所に寄せる。
- [ ] `execute_invocation` の6連 `if let` を単一の `BuiltinKind`
      解決に置き換える。戻り値を `Flow { Done, Script }` にする。
- [ ] `Invocation` を `new(id, source)` + builder に縮約し、`CommandSource` /
      `Owner` の対応を1関数にまとめる。
- [ ] `run_pane_resize_command` の生文字列解釈を `ResizeArg::parse` として
      `vy-app` に移し、テストを追加する。
- [ ] テスト: `ResizeArg::parse`（`+3` / `-2` / `50` / `0.3` / 不正値）、overlay
      のキー処理の共通部分。

完了条件: 同じ形の `match` が複数回現れない。コマンド ID
の解決順序が1つの関数で読める。

### Phase F: 構造の受け皿（任意）

- [ ] `PaneViewport { scroll_top, scroll_left, scroll_sub_line, rows, cols }`
      を導入し、`clamp_active_views` / `clamp_views_for_document` /
      `reset_views_for_document` / `ensure_cursor_visible`
      をこの型のメソッドへ移す。
- [ ] `activate_active_view` / `store_active_view`
      を1つのアクセサ（`PaneViewport` と `Editor` の対）に統合する。
- [ ] `EditorView` のフィールドを `Session` / `Scripts` / `Ui` / `Model`
      に分割する。
- [ ] `fallback`
      ドキュメント依存を減らし、「常に1ペインが存在する」不変条件を型で表す。

完了条件: `EditorView` のフィールド数が減り、`active_pane` の有無を毎回 `if let`
で確認しない。

## 5. 検証

### 自動テスト

    cargo test -p vy-app --lib      # フェーズごと
    cargo test --workspace          # 完了時
    cargo clippy --workspace --all-targets
    cargo fmt --all --check
    pnpm check:types                # API 型に触れた場合のみ

新規テスト:

- `VisualIndex`: 往復一致、折り返し跨ぎ、全角・タブ、空行、最終行、Select
  の行末クランプ
- `PendingInput`: 演算子 → モーション、演算子 →
  テキストオブジェクト、二重演算子、Esc キャンセル
- `Prefix`: カウント、レジスタ、シーケンス、タイムアウト、`0` の特別扱い
- `SessionState`: 切断 → 再接続中 → 復元 → 準備完了
- `ResizeArg::parse`: 符号付き、割合、パーセント、不正値

### 手動確認

1. 折り返し ON の長い行で `j` / `k` を連続入力し、表示桁が保たれること。
2. 折り返し ON でホイールを回し、`scroll_sub_line` が破綻しないこと。
3. IME
   の前編集が、折り返し・横スクロール・サブ行のいずれでも正しい位置に出ること。
4. `d` / `y` + テキストオブジェクト、`dd`、`2dd`、`.` の再現、`"a` レジスタ。
5. `Ctrl-w` 系のプレフィックス、`g` 系のシーケンス、タイムアウト。
6. 接続切断 → 再接続 → セッション復元、プラグイン承認 → 有効化。

### 規約確認

    rg -n '[ぁ-んァ-ヶ一-龠]' crates

## 6. リスクと対処

| リスク                                                               | 対処                                                                                                          |
| :------------------------------------------------------------------- | :------------------------------------------------------------------------------------------------------------ |
| 視覚行の置換でカーソルやスクロールの微差が出る                       | Phase A は「同じ入力で同じ結果」をテストで固定してから差し替える（往復テストとスナップショット的な期待値）    |
| `PendingInput` への移行で隠れた仕様（`cc`、`dd`、`gv` など）が壊れる | `editor/tests.rs` の既存 51 件を先に実行し、失敗を1つずつ潰す。判断が変わったものはテストと仕様書に理由を書く |
| GPUI タスクの起動順が変わり、プラグインが起動しない                  | Phase D は「状態型の導入」と「pump 起動の置き換え」を別コミットに分ける                                       |
| 変更範囲が広がりレビューできない                                     | フェーズ単位でコミットし、差分の大半を「削除」にする。新規ファイルは 3 つまで                                 |

## 7. 対象外

実行時・基盤・セキュリティ側の負債は
[`0006-runtime-safety-and-infrastructure.md`](./0006-runtime-safety-and-infrastructure.md)
で扱う。本提案の対象外は次のとおり。

- レジスタのワークスペース共有（別作業）
- `editor.tabStop` 設定の追加（この計画では `VisualOptions` に `tab_stop`
  を通せる形にしておくだけ）
- ファイル保存の原子性、ケイパビリティのパス正規化、ウィンドウ終了時の未保存保護
  （0006 で扱う）
- GPUI のバージョン更新、レンダリング性能の追加最適化

## 8. 未確定事項

- Phase D–F まで一度に実施するか、Phase A–C で区切るか。
- `VisualIndex` の置き場所は `editor/visual.rs` 新設か `text_layout.rs`
  拡張か（推奨: 新設。`text_layout` はセル幅計算に専念させる）。
- `ViewState.selection` を削除できるか（`Select` の1文字拡張と `SelectLine`
  の行単位範囲は本文が必要なため、完全な純粋導出は難しい）。
- `state.rs` の `PaneState` と `EditorView::fallback` の整理を Phase F
  に含めるか。

## 9. 関連ドキュメント

- [`../specs/architecture.md`](../specs/architecture.md):
  状態の所在（本文・カーソル・選択の分担）
- [`../specs/keymap.md`](../specs/keymap.md): キー入力の意味論と未実装キー
- [`../specs/panes.md`](../specs/panes.md): 折り返し・スクロール・ペイン状態
- [`../specs/ui.md`](../specs/ui.md): 入力ルーティングとオーバーレイ
- [`../worklogs/2026-10-editor-ui-improvements.md`](../worklogs/2026-10-editor-ui-improvements.md):
  先行した表示・性能改善の記録
