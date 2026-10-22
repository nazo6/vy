# 全体アーキテクチャ設計

Vy は、Vimのモーダル編集の軽快さと、VS Codeの拡張性・リッチUI体験を両立するため、**「GPUIフロントエンド」** と **「エディタコア/バックエンド」** を完全に物理分離するクライアント/サーバー構成を採用しています。

---

## 1. システム全体図

```mermaid
flowchart TB
    subgraph Frontend["Local Client (手元のPC)"]
        direction TB
        GPUI["GPUI Native Window (GPU 120fps Render)"]
        Webview["Embedded Webview (gpui-kit/webview)\n(Notebook出力, Markdownプレビュー, リッチUI)"]
        InputHandler["Keymap Engine (Helix風 Selection -> Action)\n(ローカル即時描画: 0ms)"]
        QuickJS_Front["QuickJS Runtime (init.ts & UI Plugins)"]
    end

    subgraph Core["Editor Core / Server (Local or Remote)"]
        direction TB
        RPC["RPC Dispatcher (Pure Rust MessagePack)"]
        Buffer["Rope Buffer & Document State (ropey)"]
        TreeSitter["Tree-sitter Engine (構文解析, ASTハイライト)"]
        LSP["Language Server Client (プロセス管理, didChange)"]
        GitWatcher["Git Status Watcher (ガター変更検知)"]
        QuickJS_Back["QuickJS Runtime (Heavy Backend Plugins)"]
    end

    InputHandler -->|即時入力・カーソル移動| GPUI
    QuickJS_Front -->|DOM/Webview制御| Webview
    QuickJS_Front <-->|UIイベント| GPUI
    
    GPUI <===>|MessagePack RPC (stdio / Socket)| RPC
    RPC <--> Buffer
    RPC <--> TreeSitter
    RPC <--> LSP
    RPC <--> GitWatcher
    RPC <--> QuickJS_Back
```

---

## 2. エディタ本体（Core）とプラグインの責務境界

| 機能 | 担当 | 理由 |
| :--- | :--- | :--- |
| **LSPクライアント基盤** | **エディタ本体 (Rust)** | プロセス起動・JSON-RPC通信・ドキュメント同期・診断エラー集約をRustネイティブで堅牢に実行。 |
| **Tree-sitter (構文解析)** | **エディタ本体 (Rust)** | ハイライト描画、インクリメンタルパース、ASTテキストオブジェクトをRustの描画ループと直結。 |
| **Rope & アンドゥ履歴** | **エディタ本体 (Rust)** | `ropey` によるテキスト編集とツリー型Undo/Redo。 |
| **Git状態の監視** | **エディタ本体 (Rust)** | ファイル変更監視と連動したガターのDiff表示。 |
| **設定・キーバインド・UI拡張** | **プラグイン/設定 (TypeScript)** | `init.ts` で柔軟にキー割り当てや外観を制御。 |

---

## 3. UIレンダリング: GPUI + `gpui-kit/webview`

- **メイン画面**: Zedが開発した **GPUI** を採用。WGPUベースのハードウェアアクセラレーションにより、120Hz/240Hzディスプレイでも遅延ゼロの描画を実現。
- **リッチUI / Webview**: [`gpui-kit/crates/webview`](https://github.com/longbridge/gpui-kit/tree/main/crates/webview) を組み込み、OSネイティブのWebview（`wry`ベース）をGPUIのレイアウトツリー内にオーバーレイ配置。
  - Jupyter Notebookのセル出力
  - Markdown / HTMLプレビュー
  - グラフ・画像のインライン表示
