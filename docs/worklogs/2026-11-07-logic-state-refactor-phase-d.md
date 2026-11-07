---
title: "セッションとプラグインのライフサイクル整理ログ"
type: "worklog"
date: 2026-11-07
phase: "Phase D"
tags: ["refactoring", "state-machine", "session", "plugin", "lifecycle"]
---

# [Worklog] セッションとプラグインのライフサイクル整理ログ

- **日付**: 2026-11-07
- **関連フェーズ/タスク**: [`0030-logic-state-refactor.md`](../adr/0030-logic-state-refactor.md) Phase D
- **目的**: `vy-app` に散在していた接続状態フラグ（`connection_generation`, `reconnecting`, `reconnect_requested`）とプラグインランタイムの起動管理（16ms ポーリングループ、`started` フラグ）を明示的な純粋状態型（`SessionState`, `RuntimeSlot`）に集約し、イベント駆動で即時かつ安全に遷移させる。

## 進め方

1. **`crates/vy-app/src/session.rs` の新設**:
   - `SessionState`（`Disconnected`, `Connecting`, `Ready`, `Restoring`）と `RuntimeSlot`（`Spawning`, `Running`, `Stopped`）を定義。
   - `InitializeParams` 構築の集約関数 `build_initialize_params` / `initialize_request` を提供。
   - GPUI 非依存でライフサイクル状態遷移を検証する単体テスト 3 件を実装。
2. **`view/rpc.rs` と `view/mod.rs` のセッション管理集約**:
   - `EditorView` に `session_state: SessionState` を導入。
   - `connect` と `try_reconnect` で重複していた `InitializeParams` 生成を `session::build_initialize_params` に統一。
   - `start_config_watch` 内の 500ms ポーリングによる `reconnect_requested` チェックを廃止し、`reconnect_requested` フィールドを完全撤廃。
   - `UiCommand::ReconnectServer` から直接再接続（`reconnect_now`）を実行できるように整理。
3. **`view/plugin.rs` のポンプ統合とポーリング廃止**:
   - `PluginRuntime` の `started: bool` と `generation: u64` を `slot: RuntimeSlot` に集約。
   - `start_script` と `start_plugin_script` を単一の統合関数 `start_host_pumps` に一本化。
   - 16ms 間隔で CPU を消費していた `start_plugin_supervisor` ループを完全削除。
   - ランタイム生成時（`reload_plugin`, `init_plugins` 等）に即時ポンプを開始するよう整理。

## 作業内容

- `crates/vy-app/src/session.rs` を新規作成し、`crates/vy-app/src/lib.rs` で公開。
- `PluginRuntime` を `RuntimeSlot` ベースにリファクタリング。
- `commands.rs` の `drain_command_queue_with_cx` を整備し、キー入力やパレット実行時（`on_key_down`）に同期的にコマンドドレインと未起動プラグインの pump 起動を実行。
- `view/rpc.rs` での接続開始・復元中・復元完了・切断時の状態遷移を `self.session_state` に反映。
- `SessionState` と `RuntimeSlot` の単体テスト 3 件を追加し、既存テスト 282 件を含む計 285 件のテストが通過することを確認。
- ワークスペース全体の単体テスト 389 件（`cargo test --workspace --lib`）、`cargo clippy`、`cargo fmt`、`pnpm format:check`、`pnpm check:types`、`cargo run -p vy-api-codegen -- --check` がすべて警告・エラーなしで合格することを確認。

## 発生した課題と対処

- **GPUI `Context` の再借用（reborrow）問題**:
  - `commands.rs` の `drain_command_queue` ループ内で `execute_invocation` を呼び出す際、`Option<&mut Context<Self>>` に対して `as_deref_mut()` を呼ぶと `Context` の `DerefMut<Target = App>` が優先されて型が `Option<&mut App>` になってしまう問題が発生した。
  - **対処**: `drain_command_queue`（GPUI 非依存）と `drain_command_queue_with_cx(&mut self, cx: &mut Context<Self>)` に明確に分離し、ループ内では `execute_invocation_with_cx(invocation, cx)` を直接呼ぶことで、Rust コンパイラの自動 re-borrow 機構を活用して安全に解決した。
- **Rust 2024 エディション予約語 `gen`**:
  - `let gen = self.next_plugin_generation;` が予約語エラーとなったため、明示的な変数名 `let generation = ...` に修正した。
