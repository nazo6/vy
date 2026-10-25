---
id: "0003"
title: "Phase 8: Webview・Notebook"
type: "proposal"
status: "open"
date: 2026-10-25
related_adrs: ["0001", "0005"]
tags: ["webview", "notebook", "preview"]
---

# [Proposal] Phase 8: Webview・Notebook

Markdown プレビューや Jupyter Notebook などの Web 文書・リッチコンテンツを表示するための最終段階の実装計画です。

---

## 1. 方針と位置づけ

ADR 0005 に基づき、エディタの操作 UI（ファイラー、パレット、設定等）はすべてネイティブのウィジェット木で構築します。Webview は、Web 文書そのものを表示する必要がある場合（Markdown プレビュー、HTML プレビュー、Jupyter Notebook 出力セルなど）に限定して使用します。

## 2. 検討項目・作業スコープ

- `gpui-kit/crates/webview` の組み込み
- Webview ペインのフォーカス管理とキー入力受け渡し
- テーマ（ダーク / ライト、カラーパレット）の Webview への注入
- Markdown ライブプレビューペインの実装
- Jupyter Notebook (`.ipynb`) のセル描画、入力セルと Webview 出力セルのハイブリッド表示

## 3. 未決定の論点

- Webview とネイティブペイン間のフォーカス切り替え時のショートカット競合解消
- Webview 内のリンククリックや外部ブラウザ起動のセキュリティポリシー

