---
title: "作業ログ (Worklogs)"
type: "guide"
updated_at: 2026-10-27
---

# 作業ログ (Worklogs)

アーキテクチャの意思決定（ADR）ではないものの、**着手した**規模の大きな作業（リファクタリング、段階的移行、パフォーマンス改善、調査検証など）の計画と実施ログを記録する場所です。未着手の提案・計画は [`../adr/`](../adr/README.md) に置きます。

ADR に書かれていた実装記録（検証した範囲、移行の各段階、つまずき）は、決定内容を残したままこのディレクトリへ移します。worklog は決定の根拠にはなりません。決定は ADR、規範は [`../specs/`](../specs/README.md) を参照してください。

設計ドキュメントや仕様書が作業詳細メモで肥大化するのを防ぎ、後から「なぜ・どのようにその作業を進めたか」「どのような課題が発生してどう解決したか」を追跡できるようにします。

## 作業ログ一覧

- [`2026-10-initial-phases.md`](./2026-10-initial-phases.md): Phase 1〜5 実装進捗ログ
- [`2026-10-editor-ui-improvements.md`](./2026-10-editor-ui-improvements.md): UI・エディタ品質改善ログ
- [`2026-10-runtime-safety-infrastructure.md`](./2026-10-runtime-safety-infrastructure.md): 実行時・基盤・セキュリティ負債整理ログ
- [`2026-10-26-logic-state-refactor.md`](./2026-10-26-logic-state-refactor.md): 純粋ロジックの状態モデル整理ログ（Phase 0–C 完了）
- [`2026-10-24-plugin-api-pipeline.md`](./2026-10-24-plugin-api-pipeline.md): プラグイン API の型・宣言・生成パイプライン整備ログ

## 命名規則

ファイル名は `YYYY-MM-DD-<slug>.md` とします（例: `2026-10-25-phase2-refactoring.md`）。

## frontmatter テンプレート

```yaml
---
title: "作業ログのタイトル"
type: "worklog"
date: 2026-10-25
phase: "Phase 2" # 任意: 関連するフェーズ
tags: ["refactoring", "gpui"]
---
```

## 本文の構成テンプレート

```markdown
# [Worklog] <作業タイトル>

- **日付**: 2026-10-25
- **関連フェーズ/タスク**: Phase 2
- **目的**: 何のためにこの作業を行うか

## 作業内容

- 実施した変更の手順や要約

## 発生した課題と対処

- 作業中につまずいた点や、予定から変更した点

## 検証結果・測定値

- テスト結果、ベンチマークやパフォーマンスの実測値など

## フォローアップ・残課題

- 次の作業に引き継ぐ項目
```
