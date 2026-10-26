---
title: "純粋ロジックの状態モデル整理ログ"
type: "worklog"
date: 2026-10-26
phase: "Phase 0"
tags: ["refactoring", "state-machine", "editor", "keymap", "text-layout"]
---

# [Worklog] 純粋ロジックの状態モデル整理ログ

- **日付**: 2026-10-26
- **関連フェーズ/タスク**: [`0005-logic-state-refactor.md`](../proposals/0005-logic-state-refactor.md) Phase 0
- **目的**: GPUI 非依存のロジックに散在する暗黙の状態と重複した視覚行計算を、挙動を変えずに整理する。まず Phase 0 で、後続フェーズの前提になる小さな不整合を直す。

## 進め方

- 第 1 系列は Phase 0 → A → B → C とし、1 フェーズ 1 PR で進める。Phase D–F（接続・プラグイン・dispatch）は別系列にする。
- Phase A の `VisualIndex` は `crates/vy-app/src/editor/visual.rs` に新設し、`text_layout` はセル幅計算とキャッシュに専念させる。
- `ViewState.selection` は本文依存（`Select` の 1 文字拡張、`SelectLine` の行単位範囲）のためフィールドとして残し、`selection == range_for(mode, anchor, head)` の不変条件を書き込み側で守る。

## 作業内容

### Phase 0

- `ViewState` から派生値の `selection` を削除し、描画直前に `Editor::range_for(mode, anchor, head)` で求めるようにした。`view_state()` / `clamp_view_state()` / `set_view_state()` の間で選択範囲の更新規則が揃い、古い本文に対する範囲が `PaneState` に残らなくなった。
- 本番コードの `chunk_line(...)` を `cached_chunk_line(...)` に置き換え、`Arc<LineChunks>` を使い回すようにした（`view/input.rs` 4 箇所、`view/render.rs` 4 箇所、`view/mod.rs` 2 箇所）。可視行ごとの `Vec` の clone をなくした。
- 本番モジュールに残っていた `chunk_line` / `chunk_line_with_tab_stop` は削除し、テストが必要とする所有 `Vec` 版は `text_layout` の `mod tests` 内のローカルヘルパーにした。テスト用の API を本番の名前空間に置かない。
- `active_register` が早期 return の経路で残らないことをテストで固定した。

## 発生した課題と対処

- `ViewState.selection` は、`Select` が head の次の文字まで広げ、`SelectLine` が本文の行境界を使うため、`mode` / `anchor` / `head` だけからは導出できない。フィールドを残す案もあったが、`PaneState` に複製された時点で古い本文の範囲が残りうるため、**フィールドを削除**し、描画直前に `Editor::range_for(mode, anchor, head)` で求める形にした。`ui/pane.rs` の `EditorPaintData` に `selection` を追加し、`prepare_editor_paint` が現在の本文から計算する。`ViewState::cursor()` は Normal のとき `anchor.min(head)` を使う。
- `chunk_line` は `Arc<LineChunks>` ではなく `Vec<LineChunk>` を返すため、置換時に `.chunks` を明示的にたどる必要があった。`LineChunks` に `Deref` を実装する案は採らず、呼び出し側で `let line_chunks = ...; let chunks = &line_chunks.chunks;` とした。
- テスト用に `ViewState` を直接組み立てる箇所で clippy の `field_reassign_with_default` が出たため、struct update 構文（`..ViewState::default()`）に直した。

## 検証結果・測定値

- `cargo test --workspace`: 全テスト緑（`vy-app` の lib は 196 → 200 件。追加は ViewState の 3 件とレジスタの 1 件）。
- `cargo clippy --workspace --all-targets`: 警告なし。
- `cargo fmt --all --check`, `pnpm format:check`: 緑。
- `rg -n '[ぁ-んァ-ヶ一-龠]' crates`: テストデータ以外のヒットなし。
- 本番コードの `chunk_line(` 呼び出しは 0（`rg 'chunk_line\(' crates/vy-app/src` でテストと定義のみ）。

## フォローアップ・残課題

- Phase D で `reconnect_requested`（`view/commands.rs` で設定し、`view/mod.rs` の 500ms ポーリングが消費する）を `SessionState` に置き換える。
- Phase A で `editor/visual.rs` を新設し、`target_display_vertical` / `on_scroll_wheel` / `view/input.rs` / `view/render.rs` / `ui/pane.rs` の視覚行計算を 1 実装へ寄せる。
- `EditorPaintData.selection` は描画のたびに `range_for` を呼ぶ。Phase A の `VisualIndex` 導入時に、位置から範囲を作る処理も同じ型へ寄せるか検討する。
