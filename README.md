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
- [**開発方針 (`docs/development.md`)**](./docs/development.md)
  - 言語、エラー、状態の所在、ビルドと検証の決まり
- [**アーキテクチャ決定記録 (`docs/adr/`)**](./docs/adr/README.md)
  - 設計上の決定と、見直しの条件

---

## 🛠 開発

```sh
# ビルドとテスト
cargo test --workspace

# クライアントを起動する。引数は開くファイル（省略時は vy:untitled）
cargo run -p vy-app -- README.md
```

- `vy-app` は、実行ファイルと同じディレクトリか `PATH` から `vy-server`
  を探します。別の場所を使うときは `VY_SERVER=/path/to/vy-server` を指定します。
- `vy-server` のログは標準エラーに出ます。量を変えるときは `VY_LOG=debug`
  を使います。
- GPUI 0.2.2 が引く `xattr 0.2.3` は Linux でビルドできないため、`vendor/xattr`
  にパッチ版を置いています（[`vendor/xattr/PATCH.md`](./vendor/xattr/PATCH.md)）。
- `VY_FRAME_LOG=1`
  で起動すると、フレームの間隔と要素の組み立て時間を標準エラーに
  出します。キー入力の重さを測るときに使います。
- GPUI と wgpu は最適化なしだと 1 フレームに数十 ms かかります。`Cargo.toml` の
  `[profile.dev.package."*"]` で依存だけ最適化しています。依存を変えた直後は
  ビルドが長くなります。

### 操作（Phase 1）

| モード | キー                                                                   |
| :----- | :--------------------------------------------------------------------- |
| Normal | `h j k l` 移動、`w b e` 単語、`0 $` 行頭と行末、`g g` / `G` 先頭と末尾 |
| Normal | `x` `d` 選択の削除、`v` 選択モード、`i` `a` 挿入、`u` / `Ctrl-r` 履歴  |
| Insert | 文字入力、`Enter` 改行、`Backspace` 削除、`Esc` で Normal へ           |
| Select | 移動で選択を伸ばす。`d` で削除して Normal へ                           |

カウント、レジスタ、`.` の繰り返し、IME の前編集はまだありません。
