---
id: "0026"
title: "Phase 6: リモートプラグイン"
type: "adr"
status: "proposed"
category: "roadmap"
date: 2026-10-25
related_adrs: ["0010", "0011", "0019"]
tags: ["remote", "plugins", "backend", "quickjs"]
---

# ADR 0026: Phase 6: リモートプラグイン

- 状態: 提案中 (proposed)
- 日付: 2026-10-25
- カテゴリー: roadmap

`backend.ts` を接続先のコア（`vy-server`）で動かし、フロントエンドとはチャネルで通信する設計は [ADR 0011](./0011-backend-plugins-on-core.md) で決定済みです。本 ADR は、その実装スコープと未決定の論点、作業計画を扱います。

---

## 1. 背景・目的

リモート開発環境（SSH 接続先など）でファイル検索、Git 連携、LSP などの重い処理やホスト依存の処理を行う場合、クライアント側ではなく接続先のサーバー（コア）側でプラグインを実行できる必要があります。

## 2. 検討項目・作業スコープ

### ① ランタイム基盤

- `vy-server` への `vy-js` 組み込みと、プラグインごとの QuickJS ランタイム分離
- プラグインライフサイクル管理（`plugin/load`、`plugin/unload`、`plugin/message`）
- バンドルの転送・SHA-256 ハッシュとサイズ検証、メモリキャッシュ

### ② 通信チャネルと API

- クライアントとバックエンド間のチャネル通信（`backend.connect` / `backend.registerHandler`）
- コア側 API の提供: `workspace`, `fs`, `process`, `config`, `log`
- コア側でのケイパビリティ検査、実行時間・メモリ上限の適用
- backend 専用の TypeScript 型定義の分離（UI 関連 API を含まないクリーンな境界）

### ③ 参照実装

- 検索プラグイン（リモートの ripgrep 呼び出しと結果ストリーム）

## 3. 未決定の論点

- **先読み描画の範囲**: リモート接続で入力の遅延を隠すため、クライアントがどこまで楽観的・先行描画を行うか（`docs/specs/architecture.md` より）。
- **プラグインのインストール先**: 設定ディレクトリと分けるか、書き込み権限のない環境を考慮してデータディレクトリに置くか（`docs/specs/layout.md` より）。
- **サーバープロセスの多重化**: 1 つの `vy-server` で複数ワークスペースを扱うか、ワークスペースごとにプロセスを分けるか。

