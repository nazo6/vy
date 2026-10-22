# Vy (ヴァイ)

> **次世代モーダルGUIエディタ**\
> Helix風の直感的なモーダル操作、GPUIによるネイティブ描画、Webview統合によるリッチコンテンツ、そしてリモート開発を融合したエディタ。

---

## 🎯 コアコンセプト

1. **直感的なモーダル編集 (Helix-inspired)**
   - `Selection -> Action`（選択してから操作）による視覚的で直感的な編集体験と、複数カーソルとの自然な親和性。
2. **GPUIネイティブGUI × Webview統合**
   - Zedが開発したGPUアクセラレーションフレームワーク **GPUI**
     による軽快な描画。性能の目標は入力遅延と大きなファイルのスクロールに置いています（[ADR 0001](./docs/adr/0001-ui-framework.md)）。
   - [`gpui-kit/webview`](https://github.com/longbridge/gpui-kit/tree/main/crates/webview)
     によるネイティブWebviewの埋め込み（Notebook、Markdownプレビュー、リッチUI）。
   - プラグインUIはネイティブのウィジェット木を既定にし、WebviewはWeb文書の表示に限ります。プラグインUIでもvimのキーバインドをそのまま使えるようにするためです（[ADR 0005](./docs/adr/0005-native-plugin-ui.md)）。
3. **フロント主導のTypeScript設定 & プラグイン (`init.ts`)**
   - 設定は TypeScript の `init.ts` と JSON の `settings.json`
     に分ける。`rquickjs` と `oxc_transform` により、`init.ts`
     をコンパイルせずに評価する。
   - 透過的プロキシAPIにより、フロントのJSからリモートのプロセスやファイルを自然に操作。
4. **Pure Rust MessagePack による極小遅延リモート開発**
   - C言語依存ゼロの `rmp-serde` によるバイナリRPC通信。
   - ヘッドレスサーバーと GUI
     クライアントを常に別プロセスにします。ローカルでは子プロセス、リモートでは
     SSH 越しにつなぎます（[ADR 0002](./docs/adr/0002-process-model.md)）。

---

## 📚 ドキュメント構成

詳細な設計ドキュメントは [`docs/`](./docs) 配下にあります。

- [**全体アーキテクチャ設計 (`docs/architecture.md`)**](./docs/architecture.md)
  - GPUIフロントエンドとコアの分離、エディタ本体とプラグインの責務分担
- [**プラグインシステムと設定 (`docs/plugin-system.md`)**](./docs/plugin-system.md)
  - `init.ts` と `settings.json`
    の役割、実行モデル、マニフェスト、ケイパビリティの承認
- [**プラグイン・設定 API 一覧 (`docs/api.md`)**](./docs/api.md)
  - 名前空間ごとの API、実行場所、必要なケイパビリティ、実装の優先度
- [**ペインとレイアウト (`docs/panes.md`)**](./docs/panes.md)
  - ウィンドウ、ペイン、ビューのモデル、操作の意味論、API、メッセージ、保存の単位
- [**プラグイン UI (`docs/ui.md`)**](./docs/ui.md)
  - ネイティブのウィジェット木、キー入力とフォーカス、アクション、Webview
    の位置づけ
- [**ファイルとフォルダの構成 (`docs/layout.md`)**](./docs/layout.md)
  - ユーザー設定、プロジェクト、プラグインのパッケージ、リポジトリの構成
- [**通信プロトコル仕様 (`docs/protocol.md`)**](./docs/protocol.md)
  - Pure Rust `rmp-serde` によるMessagePackバイナリRPCメッセージ構造
- [**ロードマップ (`docs/roadmap.md`)**](./docs/roadmap.md)
  - 実装フェーズ（Phase 1〜5）と、決定済みの項目、残っている論点
- [**アーキテクチャ決定記録 (`docs/adr/`)**](./docs/adr/README.md)
  - 設計上の決定と、見直しの条件
