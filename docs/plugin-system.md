# プラグインシステム仕様

Vy は、Vimのような柔軟な拡張性を維持しながら、Luaのような型の緩さやWASMのコンパイル負担を排除した、**「TypeScript直接実行による型安全なプラグイン基盤」** を提供します。

---

## 1. ランタイム構成: `rquickjs` + `oxc_transform`

- **実行エンジン**: Fabrice Bellard氏作の超軽量JSエンジン QuickJS をRustに統合（[`rquickjs`](https://github.com/DelSkayn/rquickjs)）。
- **TypeScript即時コンパイル**: プラグイン読込時に、Rust製超高速コンパイラ [`oxc_transform`](https://github.com/oxc-project/oxc) を挟み、**0.1ms未満で型注釈を除去して即座に実行**。
- **スペック**:
  - バイナリサイズ増加: **約 +3 MB**
  - メモリ消費: **約 2 MB / インスタンス**
  - 起動時間: **1 ms 未満**

ユーザーはビルドステップ（npm buildやtsc）を挟むことなく、**`.ts` ファイルを保存するだけで即座にリロード** できます。

---

## 2. アーキテクチャとAPIパターン

### (A) プロバイダーパターン (Provider Pattern)
単なるコマンド実行だけでなく、LSPやコード補完などの常駐型言語機能を型安全に提供します。

```typescript
import { editor, CompletionItem, CompletionContext, Position } from "vy-api";

// 補完プロバイダーの登録例
editor.languages.registerCompletionProvider({
  language: "rust",
  triggerCharacters: [".", "::"],
  async provideCompletions(document, position, context) {
    return [
      { label: "clone()", insertText: "clone()", kind: "method" },
      { label: "to_string()", insertText: "to_string()", kind: "method" }
    ];
  }
});
```

### (B) 宣言的ステートフルウィジェット (UI抽象化)
プラグインがUI描画ループを直接抱えるのではなく、**「構造」** と **「データ」** のみを宣言します。
- 検索バーのタイピングや、リストの上下移動（`<C-n>`, `<C-p>`）は手元のローカルクライアントがネイティブに0msで即時描画します。
- これにより、プラグインがリモートサーバー側で動作していても、操作のカクつきやネットワーク遅延が一切発生しません。

---

## 3. 実装例: Telescope風ファジーファインダー

```typescript
import { editor, FloatingWindow, Process } from "vy-api";

editor.registerCommand("find_files", async () => {
  let activeProcess: Process | null = null;

  // 1. フローティングウィンドウを開く (TUI/GUI共通の宣言的レイアウト)
  const modal = await editor.ui.openFloatingWindow({
    title: "Find Files (ripgrep)",
    width: "80%",
    height: "60%",
    layout: {
      type: "split",
      direction: "horizontal",
      left: { type: "list", id: "result_list" },
      right: { type: "preview", id: "preview_pane" }
    }
  });

  // 2. 検索バーの入力イベント (50msデバウンス)
  modal.onInput(async (query: string) => {
    if (activeProcess) activeProcess.kill();
    modal.clearList("result_list");
    if (!query) return;

    // リモートサーバー側で ripgrep を非同期ストリーミング実行
    activeProcess = editor.process.spawn("rg", [
      "--line-number",
      "--column",
      "--smart-case",
      query
    ]);

    activeProcess.onLine((line: string) => {
      const parsed = parseRipgrepOutput(line);
      modal.appendListItem("result_list", {
        label: `${parsed.file}:${parsed.line}  ${parsed.text}`,
        data: parsed
      });
    });
  });

  // 3. 選択項目のフォーカス移動時にプレビュー更新
  modal.onFocusItem("result_list", (item) => {
    modal.updatePreview("preview_pane", {
      filePath: item.data.file,
      highlightLine: item.data.line
    });
  });

  // 4. 決定キー (<CR>) でファイルを開く
  modal.onSelect("result_list", (item) => {
    editor.workspace.openFile(item.data.file, {
      line: item.data.line,
      column: item.data.col
    });
    modal.close();
  });
});
```

---

## 4. これから決めること (TODO / 未決定事項)

- [ ] **プラグインの権限管理モデル (Permissions)**
  - ファイルシステムアクセス、ネットワーク通信、外部プロセス実行をDenoのようにサンドボックス許可制にするか。
- [ ] **パッケージ管理と配布形式**
  - Gitリポジトリ直のクローン（Neovim風）か、npmレジストリのような中央インフラを用意するか、GitHub Releases/OCIレジストリを利用するか。
- [ ] **WASMプラグインの併用サポート**
  - TypeScriptだけでなく、RustやZig、C++等で書かれた高負荷計算プラグイン向けにWASM (Wasmtime) もサポートするか。
