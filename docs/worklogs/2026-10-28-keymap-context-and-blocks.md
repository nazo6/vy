---
title: "キーマップコンテキストの直交化とブロック構造への移行 (ADR 0034)"
type: "worklog"
date: 2026-10-28
tags: ["keymap", "refactor", "context", "when"]
---

# [Worklog] キーマップコンテキストの直交化とブロック構造への移行 (ADR 0034)

- **日付**: 2026-10-28
- **関連 ADR**: ADR 0034
- **目的**: 平坦なキーマップ定義による重複（147件中44回の最頻条件反復、13キーの二重定義）とモーダルオーバーレイ時の背面コンテキスト漏れの解消

## 作業内容

1. **フォーカスターゲット単一化とコンテキストの直交化**:
   - `crates/vy-app/src/keymap.rs` の `PaneKind` に `Overlay` を追加。
   - `crates/vy-app/src/view/mod.rs` の `apply_key_context` を改修し、モーダルサーフェス（`overlay.is_modal()`）表示時に背面の `paneKind` を `Overlay` にマスクし、`uiRole` をサーフェス自身のウィジェット役割（`palette`/`picker`/`settings` は `list`、`ex`/`prompt`/`setting_prompt` は `textInput`、`approval` は `button`）として報告するように修正。
   - `OverlayKind::default_ui_role()` を追加し、テスト環境・直接構築時も自動フォールバック可能に。

2. **意味述語の計算プロパティ整備**:
   - `KeyContext::value` に `modal`（`self.overlay.is_modal()`）を追加。
   - `hasSelection` 述語を「選択可能ウィジェットにフォーカスがあるか（`uiRole == 'list' || uiRole == 'tree' || uiRole == 'table'`）」の計算プロパティとして提供。
   - `editorFocus` 述語を `overlay == 'none' && paneKind == 'editor'` の直交条件として整合。

3. **単層コンテキストブロック構造（`ContextBlock`）への移行**:
   - `crates/vy-app/src/keymap.rs` に `ContextBlockSpec` と `BindingTarget` を定義し、`indexmap::IndexMap` を使用して順序を決定的に保持しながらフラットな `Binding` 配列へと展開するローダー `load_default` を実装。
   - `builtin/keymaps/default.json` を ADR 0034 §3 のブロック構造（14 ブロック、完全重複ゼロの 113 バインディング）に再構成。
   - オペレータ待機中（`operatorPending`）に移動・削除キーが排他されないよう、エディタ基本操作（`editorFocus && !insertMode && !textObjectPending`）と挿入モード遷移（`i`, `a`: `editorFocus && !insertMode && !operatorPending && !textObjectPending`）を条件レベルで排他・整合。
   - Ex コマンドラインの確定キー（`enter`: `vy.ui.activate`）を Ex 固有ブロックに明示化。

4. **検証とテスト拡充**:
   - `crates/vy-app/src/keymap.rs` に到達可能コンテキストマトリクス（エディタ各種モード、UIペイン各種役割、モーダルサーフェス）のキー解決テストを追加。
   - 意味述語およびコンテキスト直交化のテストを追加。
