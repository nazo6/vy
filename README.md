# Vy (ヴァイ)

> **次世代モーダルエディタ**  
> Vimの軽快な操作性と、第一級のリモート開発体験・TypeScriptによる型安全な拡張性を両立した新世代エディタ。

---

## 🎯 コアコンセプト

1. **モーダル編集 (Vim-inspired)**
   - 完全互換に縛られず、現代的な拡張性と直感性を備えたモーダルキーバインド。
2. **第一級のリモート開発 (Client-Server分離)**
   - ヘッドレスサーバーとローカルUIクライアントをRPCで完全分離。
   - ネットワーク遅延を感じさせないローカル入力予測（Optimistic Updates）。
3. **超軽量 × 型安全なTypeScriptプラグイン**
   - `rquickjs` (QuickJS) + `oxc_transform` により、**コンパイル不要で `.ts` を直接実行**。
   - エディタ本体のバイナリ増加はわずか約+3MB、メモリ消費約2MB。
4. **TUI / GUI ハイブリッドアーキテクチャ**
   - コアと描画をRPCで切り離し、初期は軽量なTUI（Ratatui）で立ち上げ、後からリッチなGUIフロントエンドを追加可能。

---

## 📚 ドキュメント構成

詳細な設計ドキュメントは [`docs/`](./docs) 配下に分割されています。

- [**全体アーキテクチャ設計 (`docs/architecture.md`)**](./docs/architecture.md)
  - クライアント/サーバー分離、UI遅延ゼロ化の仕組み、コアデータ構造
- [**プラグインシステム仕様 (`docs/plugin-system.md`)**](./docs/plugin-system.md)
  - QuickJS+oxcのTypeScript実行基盤、Providerモデル（補完等）、Telescope風実装例
- [**通信プロトコル仕様 (`docs/protocol.md`)**](./docs/protocol.md)
  - クライアント-サーバー間のRPC通信シーケンスとデータ交換
- [**ロードマップ & 未決定事項 (`docs/roadmap.md`)**](./docs/roadmap.md)
  - 実装フェーズ、およびこれから決定していく技術選定リスト
