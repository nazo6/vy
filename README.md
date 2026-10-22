# Vy (ヴァイ)

> **次世代モーダルGUIエディタ**  
> Helix風の直感的なモーダル操作、GPUIによる圧倒的な描画速度、Webview統合によるリッチコンテンツ、そして第一級のリモート開発体験を融合した新世代エディタ。

---

## 🎯 コアコンセプト

1. **直感的なモーダル編集 (Helix-inspired)**
   - `Selection -> Action`（選択してから操作）による視覚的で直感的な編集体験と、複数カーソルとの自然な親和性。
2. **GPUIネイティブGUI × Webview統合**
   - Zedが開発したGPUアクセラレーションフレームワーク **GPUI** による120fpsの軽快な描画。
   - [`gpui-kit/webview`](https://github.com/longbridge/gpui-kit/tree/main/crates/webview) によるネイティブWebviewの埋め込み（Notebook、Markdownプレビュー、リッチUI）。
3. **フロント主導のTypeScript設定 & プラグイン (`init.ts`)**
   - 設定ファイルはすべてTypeScript。`rquickjs` + `oxc_transform` により、**コンパイル不要で直接評価**。
   - 透過的プロキシAPIにより、フロントのJSからリモートのプロセスやファイルを自然に操作。
4. **Pure Rust MessagePack による極小遅延リモート開発**
   - C言語依存ゼロの `rmp-serde` によるバイナリRPC通信。
   - ヘッドレスサーバー（リモート）とGUIクライアント（ローカル）を最適化された通信で透過的に接続。

---

## 📚 ドキュメント構成

詳細な設計ドキュメントは [`docs/`](./docs) 配下にあります。

- [**全体アーキテクチャ設計 (`docs/architecture.md`)**](./docs/architecture.md)
  - GPUIフロントエンドとコアの分離、エディタ本体とプラグインの責務分担
- [**プラグインシステム & 設定 (`docs/plugin-system.md`)**](./docs/plugin-system.md)
  - `init.ts` の仕様、フロント主導プロキシモデル、大量データ処理時のバックエンド連携
- [**通信プロトコル仕様 (`docs/protocol.md`)**](./docs/protocol.md)
  - Pure Rust `rmp-serde` によるMessagePackバイナリRPCメッセージ構造
- [**ロードマップ & 未決定事項 (`docs/roadmap.md`)**](./docs/roadmap.md)
  - GPUI優先の実装フェーズ（Phase 1〜5）と、これから決める課題リスト
