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

詳細な設計ドキュメントは [`docs/`](./docs/README.md) 配下にあります。

- [**ドキュメント案内 (`docs/README.md`)**](./docs/README.md)
  - ドキュメント全体の構造、仕様書・ADR・提案・作業ログの役割分担
- [**全体アーキテクチャ設計 (`docs/specs/architecture.md`)**](./docs/specs/architecture.md)
  - GPUIフロントエンドとコアの分離、エディタ本体とプラグインの責務分担
- [**プラグインシステムと設定 (`docs/specs/plugin-system.md`)**](./docs/specs/plugin-system.md)
  - `init.ts` と `settings.json`
    の役割、実行モデル、マニフェスト、ケイパビリティの承認
- [**プラグイン・設定 API 一覧 (`docs/specs/api.md`)**](./docs/specs/api.md)
  - 名前空間ごとの API、実行場所、必要なケイパビリティ、実装の優先度
- [**ペインとレイアウト (`docs/specs/panes.md`)**](./docs/specs/panes.md)
  - ウィンドウ、ペイン、ビューのモデル、操作の意味論、API、メッセージ、保存の単位
- [**プラグイン UI (`docs/specs/ui.md`)**](./docs/specs/ui.md)
  - ネイティブのウィジェット木、キー入力とフォーカス、アクション、Webview
    の位置づけ
- [**キーマップ一覧と実装状況 (`docs/specs/keymap.md`)**](./docs/specs/keymap.md)
  - モードごとのキー、実装済みの範囲、これから入れるキー、キーマップ API の予定
- [**コマンドと設定 (`docs/specs/commands.md`)**](./docs/specs/commands.md)
  - パレット、Ex コマンドライン、キーマップの役割分担と、設定のスコープ・`:set`
    の設計
- [**ファイルとフォルダの構成 (`docs/specs/layout.md`)**](./docs/specs/layout.md)
  - ユーザー設定、プロジェクト、プラグインのパッケージ、リポジトリの構成
- [**通信プロトコル仕様 (`docs/specs/protocol.md`)**](./docs/specs/protocol.md)
  - Pure Rust `rmp-serde` によるMessagePackバイナリRPCメッセージ構造
- [**開発方針 (`docs/development.md`)**](./docs/development.md)
  - 言語、エラー、状態の所在、ビルドと検証の決まり
- [**アーキテクチャ決定記録 (`docs/adr/`)**](./docs/adr/README.md)
  - 設計上の決定と、見直しの条件
- [**提案・設計検討 (`docs/proposals/`)**](./docs/proposals/README.md)
  - 新機能や未決定事項の検討・RFC
- [**作業ログ (`docs/worklogs/`)**](./docs/worklogs/README.md)
  - リファクタリングや移行作業の計画・実施ログ

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
- `VY_FRAME_LOG=1`
  で起動すると、フレームの間隔と要素の組み立て時間を標準エラーに
  出します。キー入力の重さを測るときに使います。
- JS API のリファレンスは `pnpm docs:api` で `target/docs/vy-api`
  に生成します。手順は [`docs/development.md`](./docs/development.md)
  にあります。
- GPUI と wgpu は最適化なしだと 1 フレームに数十 ms かかります。`Cargo.toml` の
  `[profile.dev.package."*"]` で依存だけ最適化しています。依存を変えた直後は
  ビルドが長くなります。

### 操作（Phase 1–3）

| モード | キー                                                                                                                              |
| :----- | :-------------------------------------------------------------------------------------------------------------------------------- |
| Normal | `h j k l` 移動、`w b e` 単語、`0 $` 行頭と行末、`g g` / `G` 先頭と末尾                                                            |
| Normal | `x` 削除、`d` 演算子（`dw` `dd` `diw` など）、`y` ヤンク、`Y` 行ヤンク                                                            |
| Normal | `v` 選択モード、`V` 行選択、`Ctrl-v` 矩形選択、`i` `a` `I` `A` `o` `O` 挿入、`u` / `Ctrl-r` 履歴                                  |
| Normal | `Ctrl-d` / `Ctrl-u` 半ページ送り、`zz` / `zt` / `zb` カーソル行位置合わせ                                                         |
| Normal | `p` `P` 貼り付け、`"a`–`"z` でレジスタを指定、`3j` `5x` のようにカウントを前置                                                    |
| Normal | `.` 直前の変更の繰り返し、`g v` 直前に選んだ範囲の復元                                                                            |
| Normal | `Ctrl-w v` / `Ctrl-w s` 分割、`Ctrl-w h j k l` フォーカス、`Ctrl-w q` 閉じる                                                      |
| Normal | `Space` コマンドパレット（検索、`Enter` 実行、`Esc` で閉じる）                                                                    |
| Normal | `:` Ex コマンドライン（`:e` 開く、`:w` 保存、`:sp` / `:vsp` 分割、`:q` / `:q!` 閉じる、`:set wrap` / `:set nowrap` 折り返し切替） |
| Insert | 文字入力、`Enter` 改行、`Backspace` 削除、`Esc` で Normal へ                                                                      |
| Select | 移動で選択を伸ばす。`v` / `V` / `Ctrl-v` 切替、`d` で削除、`y` でコピーして Normal へ                                             |

テキストオブジェクトは `iw` / `aw` / `ip` / `ap`、`i"` / `a"`、`i(` / `a(`
などに対応しています。

キーの全一覧と実装状況は [`docs/specs/keymap.md`](./docs/specs/keymap.md) にあります。

### 設定

`~/.config/vy/settings.json` と `<workspace>/.vy/settings.json`
を読みます。値だけの設定で、プロジェクト側にコードは置きません。

```json
{
  "ui.fontFamily": "monospace",
  "ui.fontSize": 14,
  "ui.lineHeight": 20,
  "ui.lineNumbers": "absolute",
  "ui.theme": "dark",
  "editor.wrap": false,
  "editor.transactionIdleMs": 500,
  "editor.sequenceTimeoutMs": 1000
}
```

`ui.lineNumbers` は `absolute`、`relative`、`off` のいずれかです。`ui.theme` は
`dark` または `light` です。`editor.wrap` はビューポート幅での行折り返しの有無
です。実行中は `:set wrap` / `:set nowrap`（現在のウィンドウ）や `:setglobal wrap`
（セッションの既定値）、`:set` 単体で開く Settings パレットからも変えられます。
ファイルを保存すると再読み込みします。`~/.config/vy` の場所は `VY_CONFIG_HOME`
で変更できます。

### init.ts

`~/.config/vy/init.ts` を置くと、起動時に評価します。`settings.json`
より優先され、値はメモリ上だけに反映します。保存し直すとランタイムを作り直して
再評価し、前回のコマンドとキーバインドは破棄します。

```typescript
// ~/.config/vy/init.ts
import type {} from "vy-api";

vy.config.set({ "ui.fontSize": 16, "ui.lineNumbers": "relative" });

vy.commands.register(
  {
    id: "user.uppercase-document",
    title: "Uppercase Document",
    category: "edit",
    exNames: ["upper"],
  },
  async () => {
    const document = await vy.editor.activeDocument();
    if (!document) return;
    const text = await vy.document.getText({ documentId: document.id });
    await vy.editor.edit({
      edits: [
        {
          range: {
            start: { line: 0, character: 0 },
            end: document.end,
          },
          text: text.toUpperCase(),
        },
      ],
    });
    await vy.notifications.show({ message: "Uppercased the document" });
  },
);
```

`vy.keymap.bind("space u", "user.uppercase-document", { when: "normalMode" })`
のようにキーを割り当てられます。`vy` はホストがインストールするグローバルで、
`vy-api` からの import
は書きません（[ADR 0019](./docs/adr/0019-global-vy-api.md)）。
`import type {} from "vy-api"` は型を有効にするための1行で、実行時には消えます。
評価できる API は [`docs/specs/api.md`](./docs/specs/api.md) の「Phase 2
の実装状況」にまとめました。型定義は
[`packages/vy-api/index.d.ts`](./packages/vy-api/index.d.ts) にあり、プロトコル
とプラグイン境界の型は [`packages/vy-api/types`](./packages/vy-api/types) に
Rust から生成します。`examples/init.ts` も参照してください。

初期化は 1 秒、コマンド実行は 5 秒で打ち切り、メモリは 64 MiB に制限します。
無限ループを書いてもアプリは落ちず、状態行にエラーが出ます。ログは
`~/.cache/vy/logs/init.log` に書きます。
