---
title: "実行時・基盤・セキュリティ負債整理ログ"
type: "worklog"
date: 2026-10-26
phase: "Phase 0"
tags: ["security", "data-integrity", "documentation", "commands"]
---

# [Worklog] 実行時・基盤・セキュリティ負債整理ログ

- **日付**: 2026-10-26
- **関連フェーズ/タスク**: [`0006-runtime-safety-and-infrastructure.md`](../proposals/0006-runtime-safety-and-infrastructure.md) Phase 0
- **目的**: 仕様書の記述を実装に合わせ、以降のフェーズ（特に Phase 1 の保存・終了フロー）が前提にできる状態にする。

## 作業内容

Phase 0「文書と実装の突き合わせ」として、`:q!`、`:wq` / `:qa`、`pane/resize` の記述をコードと照合し、次のファイルを更新した。

- `docs/specs/commands.md`
  - `:q!` を「初期版では扱わない」から実装済みに修正。`!` は未保存の確認を飛ばすが、ウィンドウ最後の 1 枚は `Layout::close` の `LastPane` で閉じられないことを明記。
  - Ex 名の表に `:resize` / `:res` と `:equalize` を追加。
  - `:qa` / `:qa!` / `:wqa` が未登録であること、`:wq` と併せて追加する予定を追記。
- `docs/specs/panes.md`
  - 「未保存の変更がある最後のペイン」を「その文書を表示している最後のペイン」に修正し、ウィンドウ最後の 1 枚と区別。
  - `:resize` は引数なし（高さ +5%）のみ動くことを追記。
- `docs/specs/keymap.md`
  - 認識するキーに `Delete`、`Shift-Tab`（`BackTab`）、`Ctrl` + 英字全般を追加。
  - Insert の `Delete` の備考を「キー名を受け取っていない」から「キー名は解釈するが処理がない」に修正。
  - 古いパス `editor.rs` を `editor/` に修正。
- `docs/proposals/0006-runtime-safety-and-infrastructure.md`
  - Phase 0 のチェックボックスを完了にし、確認結果を記録。
  - Phase 1 の完了条件に `:wq` / `:qa` / `:qa!` / `:wqa` と最後のペインの `:q` を追加。
  - `:resize` の引数未宣言を §3.9 と Phase 6 に追加。

## 発生した課題と対処

- `:resize` は Ex 名として登録済みだが、`PaneCommand::Resize` の `ArgSpec` が `None` のため引数を受け取れない。`run_pane_resize_command` にある `+N` / `-N` / パーセント / 比率の解釈は到達しない。Phase 0 では仕様書を実態（引数なしのみ）に合わせ、修正は Phase 6 に送った。
- `:q!` は「強制終了」という名前だが、終了ではなくペインを閉じる。ウィンドウ最後の 1 枚には効かないため、仕様書では「未保存の確認を飛ばす」と書き分けた。

## 検証結果・測定値

- コード変更なし。`pnpm format:check` で整形を確認する。
- `rg -n ':q!|:wq|pane/resize' docs` で、提案と仕様書の記述が同じ挙動を指していることを確認した。

## フォローアップ・残課題

- Phase 1: 原子的な保存、未保存時の終了確認、`:wq` / `:qa` 系の実装。
- Phase 6: `:resize` の `ArgSpec` 宣言と引数経路のテスト。
- `docs/development.md` の CI 記述は Phase 3 で実態に合わせる。
