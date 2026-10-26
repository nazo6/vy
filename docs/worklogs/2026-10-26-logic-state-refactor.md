---
title: "純粋ロジックの状態モデル整理ログ"
type: "worklog"
date: 2026-10-26
phase: "Phase 0–A"
tags: ["refactoring", "state-machine", "editor", "keymap", "text-layout"]
---

# [Worklog] 純粋ロジックの状態モデル整理ログ

- **日付**: 2026-10-26
- **関連フェーズ/タスク**: [`0005-logic-state-refactor.md`](../proposals/0005-logic-state-refactor.md) Phase 0, Phase A
- **目的**: GPUI 非依存のロジックに散在する暗黙の状態と重複した視覚行計算を、挙動を変えずに整理する。Phase 0 で後続フェーズの前提になる小さな不整合を直し、Phase A で視覚行の計算を 1 実装に寄せる。

## 進め方

- 第 1 系列は Phase 0 → A → B → C とし、1 フェーズ 1 PR で進める。Phase D–F（接続・プラグイン・dispatch）は別系列にする。
- Phase A の `VisualIndex` は `crates/vy-app/src/editor/visual.rs` に新設し、`text_layout` はセル幅計算とキャッシュに専念させる。
- `ViewState.selection` は派生値のため持たない。描画側が `Editor::range_for(mode, anchor, head)` で現在の本文から求める。

## 作業内容

### Phase 0

- `ViewState` から派生値の `selection` を削除し、描画直前に `Editor::range_for(mode, anchor, head)` で求めるようにした。`view_state()` / `clamp_view_state()` / `set_view_state()` の間で選択範囲の更新規則が揃い、古い本文に対する範囲が `PaneState` に残らなくなった。
- 本番コードの `chunk_line(...)` を `cached_chunk_line(...)` に置き換え、`Arc<LineChunks>` を使い回すようにした（`view/input.rs` 4 箇所、`view/render.rs` 4 箇所、`view/mod.rs` 2 箇所）。可視行ごとの `Vec` の clone をなくした。
- 本番モジュールに残っていた `chunk_line` / `chunk_line_with_tab_stop` は削除し、テストが必要とする所有 `Vec` 版は `text_layout` の `mod tests` 内のローカルヘルパーにした。テスト用の API を本番の名前空間に置かない。
- `active_register` が早期 return の経路で残らないことをテストで固定した。

### Phase A

- `crates/vy-app/src/editor/visual.rs` を新設した。`VisualOptions { cols: Option<usize>, tab_stop }` が折り返し幅を持ち、`cols: None` は「1 行 = 1 視覚行、列は UTF-16 オフセット」を表す。`VisualIndex` が `chunks` / `rows_in` / `row_of` / `position_at` / `display_col` / `step` / `rows_between` / `move_vertical` を提供する。
- `Editor::visual_index()`（現在の折り返し幅）と `Editor::visual_index_with(cols)`（ペインの幅を明示）を追加した。ペインはアクティブな 1 枚しか Editor に同期されないため、描画・入力は `visual_index_with` を使う。
- `editor/motion.rs` の `target_display_vertical`（約 100 行）を `VisualIndex::move_vertical` の呼び出しに置き換えた。折り返し無効時の UTF-16 列と、有効時の表示桁の違いは `VisualOptions` で表す。
- `view/render.rs` の `ensure_cursor_visible` とホバー位置計算、`view/mod.rs` の `on_scroll_wheel`、`view/input.rs` の `bounds_for_range` / `character_index_for_point`、`ui/pane.rs` の `collect_visual_rows` を `VisualIndex` に置き換えた。
- 行末クランプ（Normal は 1 文字手前、Select 系は行末）は `position_at` の 1 実装だけになった。

## 発生した課題と対処

- `ViewState.selection` は、`Select` が head の次の文字まで広げ、`SelectLine` が本文の行境界を使うため、`mode` / `anchor` / `head` だけからは導出できない。フィールドを残す案もあったが、`PaneState` に複製された時点で古い本文の範囲が残りうるため、**フィールドを削除**し、描画直前に `Editor::range_for(mode, anchor, head)` で求める形にした。`ui/pane.rs` の `EditorPaintData` に `selection` を追加し、`prepare_editor_paint` が現在の本文から計算する。`ViewState::cursor()` は Normal のとき `anchor.min(head)` を使う。
- `chunk_line` は `Arc<LineChunks>` ではなく `Vec<LineChunk>` を返すため、置換時に `.chunks` を明示的にたどる必要があった。`LineChunks` に `Deref` を実装する案は採らず、呼び出し側で `let line_chunks = ...; let chunks = &line_chunks.chunks;` とした。
- テスト用に `ViewState` を直接組み立てる箇所で clippy の `field_reassign_with_default` が出たため、struct update 構文（`..ViewState::default()`）に直した。
- 折り返し無効時の縦移動は **UTF-16 列**、有効時は **表示桁** を保つという既存の非対称がある。`VisualOptions { cols: None }` で「行 = 1 視覚行、列 = UTF-16」と定義し、`move_vertical` の戻り値の列もその意味で返すことで、呼び出し側の分岐をなくした。
- `ui/pane.rs` には描画用の `VisualRow`（行番号・チャンク・テキストを持つ）が既にあり、`vy_app::editor::VisualRow` と名前が衝突する。描画側は型を import せず `index.chunks(line)` だけを使う形にして、既存の `VisualRow` を残した。
- `ensure_cursor_visible` の「スクロール位置からカーソルまでの行数」は、`VisualRow` を 1 行ずつ進めるのではなく `rows_in` の合計で求める `rows_between` にした（行数に比例、行内チャンク数には依存しない）。
- `character_index_for_point` の「文書末尾より下をクリックしたら末尾」というフォールバックは、`VisualIndex::step` が最終行の最終チャンクで止まる性質でそのまま表現できた。

## 検証結果・測定値

- `cargo test --workspace`: 全テスト緑（`vy-app` の lib は 196 → 208 件。追加は ViewState 3 件、レジスタ 1 件、`VisualIndex` 8 件）。
- `cargo clippy --workspace --all-targets`: 警告なし。
- `cargo fmt --all --check`, `pnpm format:check`: 緑。
- `rg -n '[ぁ-んァ-ヶ一-龠]' crates`: テストデータ以外のヒットなし。
- チャンク分割の本番呼び出しは `editor/visual.rs` のみ（`rg 'cached_chunk_line|chunk_line\(' crates` で `text_layout` の定義とテスト以外は `visual.rs`）。
- 手動確認（折り返し ON の `j` / `k`、ホイール、IME 前編集、ホバー）は GUI を起動していないため未実施。単体テストの往復一致・折り返し跨ぎ・全角/タブ・空行・最終行で代替した。

## フォローアップ・残課題

- Phase B（`PendingInput`）で `Editor` / `ViewState` の `operator` / `operator_count` / `text_object` を 1 つの enum にまとめ、`active_register` を廃止する。
- Phase C（`Keymap` の `Prefix`）でプレフィックス状態を 1 つの値にする。`ActiveInput` は既にあるので、`on_key_down` の残りの分岐だけを寄せる。
- Phase D で `reconnect_requested`（`view/commands.rs` で設定し、`view/mod.rs` の 500ms ポーリングが消費する）を `SessionState` に置き換える。
- 手動確認（折り返し ON の連続 `j` / `k`、ホイール、IME 前編集、ホバー、マウス選択）を実機で行う。
- `text_layout::cursor_in_chunk` は `ui/pane.rs` の描画だけで使う。Phase A で `VisualIndex` に行の所属判定が入ったため、描画側も `row_of` に寄せられるか検討する。
