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
- [**キーマップ一覧と実装状況 (`docs/keymap.md`)**](./docs/keymap.md)
  - モードごとのキー、実装済みの範囲、これから入れるキー、キーマップ API の予定
- [**ファイルとフォルダの構成 (`docs/layout.md`)**](./docs/layout.md)
  - ユーザー設定、プロジェクト、プラグインのパッケージ、リポジトリの構成
- [**通信プロトコル仕様 (`docs/protocol.md`)**](./docs/protocol.md)
  - Pure Rust `rmp-serde` によるMessagePackバイナリRPCメッセージ構造
- [**ロードマップ (`docs/roadmap.md`)**](./docs/roadmap.md)
  - 実装フェーズ（Phase 1〜6）と、決定済みの項目、残っている論点
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

### 操作（Phase 1–2）

| モード | キー                                                                           |
| :----- | :----------------------------------------------------------------------------- |
| Normal | `h j k l` 移動、`w b e` 単語、`0 $` 行頭と行末、`g g` / `G` 先頭と末尾         |
| Normal | `x` 削除、`d` 演算子（`dw` `dd` `diw` など）、`y` ヤンク、`Y` 行ヤンク         |
| Normal | `v` 選択モード、`i` `a` 挿入、`u` / `Ctrl-r` 履歴                              |
| Normal | `p` `P` 貼り付け、`"a`–`"z` でレジスタを指定、`3j` `5x` のようにカウントを前置 |
| Normal | `.` 直前の変更の繰り返し                                                       |
| Insert | 文字入力、`Enter` 改行、`Backspace` 削除、`Esc` で Normal へ                   |
| Select | 移動で選択を伸ばす。`d` で削除、`y` でコピーして Normal へ                     |

テキストオブジェクトは `iw` / `aw` / `ip` / `ap`、`i"` / `a"`、`i(` / `a(`
などに対応しています。

キーの全一覧と実装状況は [`docs/keymap.md`](./docs/keymap.md) にあります。

### 設定

`~/.config/vy/settings.json` と `<workspace>/.vy/settings.json`
を読みます。値だけの設定で、プロジェクト側にコードは置きません。

```json
{
  "ui.fontFamily": "monospace",
  "ui.fontSize": 14,
  "ui.lineHeight": 20,
  "ui.lineNumbers": "absolute",
  "editor.transactionIdleMs": 500,
  "editor.sequenceTimeoutMs": 1000
}
```

`ui.lineNumbers` は `absolute`、`relative`、`off`
のいずれかです。ファイルを保存すると再読み込みします。`~/.config/vy` の場所は
`VY_CONFIG_HOME` で変更できます。

### init.ts（Phase 2）

`~/.config/vy/init.ts` を置くと、起動時に評価します。`settings.json`
より優先され、値はメモリ上だけに反映します。保存し直すとランタイムを作り直して
再評価し、前回のコマンドとキーバインドは破棄します。

```typescript
// ~/.config/vy/init.ts
import { commands, config, editor, notifications } from "vy-api";

config.set({ "ui.fontSize": 16, "ui.lineNumbers": "relative" });

commands.registerCommand("user.uppercase-document", async () => {
  const document = await editor.activeDocument();
  if (!document) return;
  const text = await document.getText();
  await editor.edit((builder) => {
    builder.replace(document.fullRange(), text.toUpperCase());
  });
  await notifications.show("Uppercased the document");
});
```

`keymap.bind("space u", "user.uppercase-document", { when: "normalMode" })`
のようにキーを割り当てられます。評価できる API は [`docs/api.md`](./docs/api.md)
の「Phase 2 の実装状況」にまとめました。型定義は
[`packages/vy-api/index.d.ts`](./packages/vy-api/index.d.ts) にあり、プロトコル
の型は [`packages/vy-api-types`](./packages/vy-api-types) に Rust
から生成します。`examples/init.ts` も参照してください。

初期化は 1 秒、コマンド実行は 5 秒で打ち切り、メモリは 64 MiB に制限します。
無限ループを書いてもアプリは落ちず、状態行にエラーが出ます。ログは
`~/.cache/vy/logs/init.log` に書きます。
