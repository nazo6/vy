---
title: "データ保護と終了フローの実装 (ADR 0031 Phase 1)"
type: "worklog"
date: 2026-11-07
phase: "Phase 1"
tags: ["safety", "atomic-save", "quit-flow", "commands"]
---

# [Worklog] データ保護と終了フローの実装 (ADR 0031 Phase 1)

- **日付**: 2026-11-07
- **関連フェーズ/タスク**: ADR 0031 Phase 1「データ保護と終了フロー」
- **目的**: 編集中のファイル保存時の原子的書き込み・外部変更保護と、安全なウィンドウ終了フロー（未保存保護モーダル、終了系コマンド `:wq` / `:qa` / `:qa!` / `:wqa`、最後のペインの終了フォールバック）を完遂する。

## 作業内容

1. **原子的保存ヘルパーの実装**:
   - `crates/vy-server/src/session/atomic.rs` に `atomic_write` を新設。
   - 対象ファイルの同一親ディレクトリに一時ファイル（`format!(".{name}.tmp-{pid}-{rand}")`）を作成し、書き込み後に `sync_all` を実行。既存ファイルのパーミッションを継承した上で原子的 `rename` を行う。失敗時は一時ファイルを確実に後始末する。
   - 既存の `save_document`、`fs_write_file`、`persist_session` を `atomic_write` に置き換え。

2. **外部変更検知と `:w!` 強制保存**:
   - `vy-server` の `Session` に `file_metadata: HashMap<DocumentId, FileMetadata>` を追加。ファイルオープン時および保存完了時にファイルの `mtime` と `size` を記録。
   - `vy-proto` の `DocumentSaveParams` に `force: bool`（`#[serde(default)]`）を追加し、プロトコル互換性を維持。
   - `save_document` 実行時にディスク上の現在の `mtime` / `size` と比較。外部から変更されていた場合、`force == false` なら `File modified externally (use ! to override)` エラーで保存を拒絶。`:w!`（`force == true`）なら強制保存してメタデータを更新。

3. **終了系コマンドの実装**:
   - `crates/vy-client-api/src/command.rs`:
     - `DocumentCommand::Save`: `w`, `write` に `.with_bang()` を付与。
     - `DocumentCommand::WriteQuit`: `wq`、`.with_bang()` を追加。
     - `UiCommand::QuitAll`: `qa`, `qall`、`.with_bang()` を追加。
     - `UiCommand::WriteQuitAll`: `wqa`, `wqall`、`.with_bang()` を追加。
   - `crates/vy-app/src/view/commands.rs`:
     - `run_document_command` で `DocumentCommand::Save` に `force: invocation.bang` を渡すよう更新。
     - `DocumentCommand::WriteQuit` を追加し、保存成功後にペインを閉じる（最後のペインなら終了）。
     - `UiCommand::QuitAll` を追加。未保存文書があれば警告し、なければ終了。
     - `UiCommand::WriteQuitAll` を追加。未保存文書を全件保存し、完了後に終了。
     - `close_active_pane`: ペインが残り1つの場合（`is_last_pane`）、未保存文書がなければウィンドウ終了、未保存文書があれば `:q!` 以外はエラー通知。

4. **未保存保護モーダルとウィンドウ終了の統制**:
   - `crates/vy-app/src/overlay.rs`:
     - `Overlay::ConfirmQuit` および `ConfirmQuitState` を新設。
   - `crates/vy-app/src/palette.rs`:
     - `confirm_quit_picker_items` を追加し、「Save and Quit」「Discard and Quit」「Cancel」および未保存文書一覧を表示。
   - `crates/vy-app/src/view/mod.rs`:
     - `EditorView::request_quit(&mut self) -> bool` を新設。未保存文書があればモーダルを表示して `false`（終了中断）を返し、未保存がなければ `true`（終了許可）を返す。
   - `crates/vy-app/src/ui/titlebar.rs` & `render.rs`:
     - タイトルバーの閉じるボタンで `request_quit` を呼び出すよう変更。
   - `crates/vy-app/src/main.rs`:
     - `window.on_window_should_close` を登録し、OS 起点でのウィンドウ終了要求時にも `request_quit` を経由して未保存データを保護。

## 発生した課題と対処

- **`vy-server` テストでの `buffer/did_change` の形式**:
  - `did_change` を `request`（ID 付き）で送るとサーバー側で unknown method エラー（通知専用のため）となり、バッファ変更が適用されなかった。`RpcMessage::notification` を使用する `insert_change` の形式に合わせて解決。
- **GPUI ウィンドウ終了フック**:
  - `WindowHandle` ではなく `Window` インスタンスから `on_window_should_close` を呼び出す必要がある。`main.rs` の `window.update` 内で登録することで解決。

## 検証結果・測定値

- `vy-server`: 32 単体テスト + 2 結合テスト通過（原子的保存、外部変更検知、`:w!` 強制保存を含む）。
- `vy-core`: 36 テスト全件通過。
- `vy-app`: 単体テスト通過（Ex コマンド解決、bang 解析、確認ピッカー生成を含む）。
