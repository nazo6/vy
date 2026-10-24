# プラグイン・設定 API 一覧

Vy がプラグインと設定に公開する API
を列挙します。実行する層と、必要なケイパビリティと、実装の優先度を併記します。ここに挙げた
API をすべて最初のリリースで実装するわけではありません。形だけを先に決め、実装は
Phase に分けます。

## 1. 設計方針

- 入口は名前空間の import
  にします（`import { editor, workspace } from "vy-api"`）。
- 状態の読み取りも Promise
  を返します。リモート接続でも同じ書き方にするためです。同期の get
  は用意しません。
- 位置と範囲は UTF-16 のコード単位で数えます。JS
  の文字列と同じ数え方にそろえ、コアとの境界で変換します。
- UI の拡張は、ペインとして開くビューの登録（`ui`
  ビューのウィジェット木を含む）と、スロットへのデータの提供に限ります。Webview
  は最終手段にします。GPUI の要素を JS
  から作ることはできません。サイドバーやパネルのような固定の領域は作らず、すべての領域をペインとして扱います。
- ファイル、プロセス、ネットワーク、シークレットを使う API
  は、マニフェストの宣言と初回の承認を必要とします。
- バッファの内容はコアが正とします。フロントは先に適用して `base_version`
  付きで送り、返ってきた version が合わなければ再同期します。
- 境界の型は Rust を正とします。params / reply / イベント payload 型はハンドラの隣（`crates/vy-app/src/api/namespaces`）に置き、共有する値型は `vy-app` の対応するモジュールに置きます。`packages/vy-api`（生成された型は `packages/vy-api/types`）の宣言は、その宣言から生成します（[ADR 0009](./adr/0009-api-types-live-with-handlers.md)）。

## 2. 表記の約束

実行場所の列は、状態の正を持つ層を表します。処理の実体がどちらにあるか、という意味です。呼び出し元がクライアントかバックエンドかは、別に扱います。

| 表記   | 意味                                                                |
| :----- | :------------------------------------------------------------------ |
| local  | クライアントで完結する。コアは関与しない。                          |
| remote | コアで完結する。クライアントは要求を送り、結果を受け取る。          |
| both   | コアとクライアントの両方が関わる。状態を持つ層は API ごとに異なる。 |

クライアントとコアは、ローカル接続でも別のプロセスです（[ADR 0002](./adr/0002-process-model.md)）。`editor.open`
が `both`
なのは、文書とペインをコアが作り、クライアントがそれを描くためです。ローカル接続でも、ファイルを開くのはコアのプロセスになります。

カーソル、選択、モード、スクロールはクライアントが持ちます。どの状態をどちらが持つかは
[`architecture.md`](./architecture.md) の「状態の所在」にまとめました。

どの API もクライアントから呼べます。`backend.ts` から呼べるのは、コアで完結する
API と、コアが状態の正を持つ API です。

ケイパビリティの列が「なし」の API
は、承認なしで使えます。それ以外は、マニフェストで宣言し、初回の有効化で承認を得た場合だけ動きます。組み込みプラグインは承認を省略します。

### 2.1 引数の形

パラメータを取る低レベル API は、オブジェクトを 1
つ渡します。`editor.setMode({ mode: "insert" })`、`config.get({ key: "ui.fontSize" })`、`ui.hover.show({ line, character, text })`
のように、メソッドごとの params
型がそのまま引数になります。パラメータの無いメソッドは引数を取りません（`editor.mode()`）。型は
[`packages/vy-api/types`](../packages/vy-api/types)
に生成され、メソッド名と型の対応は `HostMethods` が持ちます。

Phase 3 以降、すべてのメソッドは params オブジェクトを 1
つ受け取ります。糖衣はコールバック登録（`commands.registerCommand`）だけです。イベントは
`namespace.onEvent(listener)` で購読し、`Disposable`
を返します。以下の表に位置引数で書かれた古い形は、params
オブジェクトに読み替えてください。

優先度は Phase に対応します。

| 優先度 | 対応する Phase  | 範囲                                                                                                       |
| :----- | :-------------- | :--------------------------------------------------------------------------------------------------------- |
| P0     | Phase 1 から 3  | 文書の読み取り、ペインとビューの骨格、キーバインド、コマンド（実行とパレット）、設定、ログ、通知、保存領域 |
| P1     | Phase 3 から 4  | UI スロット（statusline, picker, gutter, hover）、検索、ファイル、プロセス、診断、装飾、実用プラグイン基盤 |
| P2     | Phase 7         | LSP、補完、スニペット、整形、Git                                                                           |
| P3     | Phase 8（最終） | Webview、Notebook、Markdown プレビュー                                                                     |

## 3. ライフサイクル

プラグインは `activate(ctx)` を公開します。`deactivate()` は省略できます。

| API                          | 実行場所 | ケイパビリティ  | 優先度 |
| :--------------------------- | :------- | :-------------- | :----- |
| `activate(ctx)`              | local    | なし            | P0     |
| `deactivate()`               | local    | なし            | P0     |
| `ctx.pluginId`               | local    | なし            | P0     |
| `ctx.version`                | local    | なし            | P0     |
| `ctx.extensionUri`           | local    | なし            | P0     |
| `ctx.subscriptions`          | local    | なし            | P0     |
| `ctx.capabilities.has(name)` | local    | なし            | P0     |
| `ctx.log`                    | local    | なし            | P0     |
| `ctx.storage.workspace`      | local    | workspace:state | P0     |
| `ctx.storage.global`         | local    | workspace:state | P0     |
| `ctx.secrets`                | local    | secrets         | P2     |

`ctx.subscriptions` に `Disposable`
を積むと、無効化のときにまとめて破棄されます。`activate` が返した Promise
が解決するまで、そのプラグインのコマンドは実行できません。

## 4. editor

| API                                          | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------------- | :------- | :------------- | :----- |
| `editor.activeDocument()`                    | both     | なし           | P0     |
| `editor.documents()`                         | both     | なし           | P0     |
| `editor.open(uri, options?)`                 | both     | なし           | P0     |
| `editor.reveal(uri, range?)`                 | local    | なし           | P1     |
| `editor.mode()`                              | local    | なし           | P0     |
| `editor.setMode({ mode })`                   | local    | なし           | P0     |
| `editor.selections()`                        | local    | なし           | P0     |
| `editor.setSelections({ selections })`       | local    | なし           | P0     |
| `editor.onDidChangeActiveDocument(listener)` | local    | なし           | P0     |
| `editor.onDidChangeSelection(listener)`      | local    | なし           | P0     |
| `editor.onDidChangeMode(listener)`           | local    | なし           | P1     |
| `editor.viewport()`                          | local    | なし           | P2     |

`editor.open` は、フォーカス中のペインで文書を開きます。別のペインで開くときは
`options.placement` を渡します。ペイン自体を作るときは `panes.open`
を使います。同じ文書を2つのペインで開いた場合、内容はコアで共有し、カーソル、選択範囲、スクロール位置はペインごとにクライアントが持ちます。

`editor.edit(builder)`
は編集の入口です。1回の呼び出しを1つのトランザクションとして扱い、複数カーソルの編集もまとめて適用します。`TextEdit`
の配列をそのまま渡すこともできます。

| API                                   | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------ | :------- | :------------- | :----- |
| `editor.edit(builder)`                | remote   | なし           | P0     |
| `builder.insert(position, text)`      | local    | なし           | P0     |
| `builder.replace(range, text)`        | local    | なし           | P0     |
| `builder.delete(range)`               | local    | なし           | P0     |
| `builder.selection(index, selection)` | local    | なし           | P1     |
| `editor.undo()`                       | remote   | なし           | P0     |
| `editor.redo()`                       | remote   | なし           | P1     |

`editor.edit` は適用後の version
と、変更された範囲を返します。フロントの表示は、返ってきた version が現在の
version と一致したときだけ確定します。

`editor.undo()` と `editor.redo()`
は、コアが持つ履歴をたどります。カーソルと選択はクライアントが持つため、`editor.edit`
は編集の前後の選択を一緒に送ります。取り消したあとの選択は、コアが返した値をクライアントが適用します。コアが起こした編集（整形、リネーム、複数ファイルの置換）も同じ履歴に入ります（[ADR 0004](./adr/0004-undo-in-core.md)）。

## 5. documents

`editor.activeDocument()` と `editor.documents()` は `Document`
オブジェクトを返します。次の API はそのオブジェクトのメソッドとプロパティです。

| API                                     | 実行場所 | ケイパビリティ | 優先度 |
| :-------------------------------------- | :------- | :------------- | :----- |
| `document.id`                           | both     | なし           | P0     |
| `document.uri`                          | both     | なし           | P0     |
| `document.languageId`                   | both     | なし           | P0     |
| `document.version`                      | both     | なし           | P0     |
| `document.getText(range?)`              | both     | なし           | P0     |
| `document.lineCount`                    | both     | なし           | P0     |
| `document.lineAt(line)`                 | both     | なし           | P0     |
| `document.positionAt(offset)`           | local    | なし           | P0     |
| `document.offsetAt(position)`           | local    | なし           | P0     |
| `document.isDirty`                      | both     | なし           | P0     |
| `document.save()`                       | both     | なし           | P0     |
| `document.eol`                          | both     | なし           | P1     |
| `document.setEol(eol)`                  | both     | なし           | P2     |
| `document.encoding`                     | both     | なし           | P2     |
| `document.onDidChangeContent(listener)` | local    | なし           | P0     |
| `document.onDidSave(listener)`          | local    | なし           | P1     |
| `document.onDidClose(listener)`         | local    | なし           | P1     |

`onDidChangeContent` は version
の昇順で届きます。イベントには変更範囲と、変更後の version が入ります。

## 6. decorations と diagnostics

| API                                                   | 実行場所 | ケイパビリティ | 優先度 |
| :---------------------------------------------------- | :------- | :------------- | :----- |
| `decorations.createTextEditorDecorationType(options)` | local    | なし           | P1     |
| `decorations.setDecorations(uri, type, ranges)`       | local    | なし           | P1     |
| `decorations.setInlayHints(uri, provider)`            | local    | なし           | P2     |
| `decorations.setGutterItems(uri, provider)`           | local    | なし           | P2     |
| `diagnostics.set(uri, items)`                         | both     | なし           | P1     |
| `diagnostics.clear(uri)`                              | both     | なし           | P1     |
| `diagnostics.onDidChangeDiagnostics(listener)`        | local    | なし           | P1     |

装飾は表示だけに影響します。バッファの内容は変えません。

## 7. workspace

| API                                                   | 実行場所 | ケイパビリティ  | 優先度 |
| :---------------------------------------------------- | :------- | :-------------- | :----- |
| `workspace.folders()`                                 | both     | なし            | P0     |
| `workspace.rootUri`                                   | both     | なし            | P0     |
| `workspace.name`                                      | both     | なし            | P0     |
| `workspace.trusted`                                   | local    | なし            | P0     |
| `workspace.textDocuments()`                           | both     | なし            | P0     |
| `workspace.state.get(key)`                            | local    | workspace:state | P0     |
| `workspace.state.update(key, value)`                  | local    | workspace:state | P0     |
| `workspace.findFiles(include, exclude?, maxResults?)` | remote   | fs:read         | P1     |
| `workspace.grep(query, options?)`                     | remote   | fs:read         | P1     |
| `workspace.applyEdit(edit)`                           | remote   | fs:write        | P1     |
| `workspace.saveAll(includeUntitled?)`                 | both     | なし            | P1     |
| `workspace.createFileSystemWatcher(glob)`             | remote   | fs:read         | P1     |
| `workspace.onDidChangeFolders(listener)`              | local    | なし            | P2     |

`workspace.grep` は結果をストリームで返します。全件を配列で返す API
は用意しません。理由は、数十万行の結果をそのまま転送すると帯域とメモリを使い切るためです。

## 8. ファイルシステム

`fs`
はワークスペースのファイルを扱います。リモート接続では、接続先のファイルを指します。クライアント側のファイルを扱うときは
`localFs` を使います。

| API                             | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------ | :------- | :------------- | :----- |
| `fs.readFile(uri)`              | remote   | fs:read        | P1     |
| `fs.writeFile(uri, data)`       | remote   | fs:write       | P1     |
| `fs.readDir(uri)`               | remote   | fs:read        | P1     |
| `fs.stat(uri)`                  | remote   | fs:read        | P1     |
| `fs.exists(uri)`                | remote   | fs:read        | P1     |
| `fs.createDir(uri)`             | remote   | fs:write       | P1     |
| `fs.remove(uri, options?)`      | remote   | fs:write       | P1     |
| `fs.rename(from, to)`           | remote   | fs:write       | P1     |
| `fs.copy(from, to)`             | remote   | fs:write       | P1     |
| `fs.watch(uri, options?)`       | remote   | fs:read        | P2     |
| `localFs.readFile(path)`        | local    | fs:local:read  | P2     |
| `localFs.writeFile(path, data)` | local    | fs:local:write | P2     |

読み取りは `Uint8Array` を返します。文字列が欲しいときは `readTextFile`
を使うか、`TextDecoder` で変換します。`fs` と `localFs`
を分けるのは、リモート開発中に画像の貼り付けやローカルの設定ファイル参照が必要になるためです。

## 9. プロセスとネットワーク

| API                                          | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------------- | :------- | :------------- | :----- |
| `process.spawn(command, args, options)`      | remote   | process:exec   | P1     |
| `process.exec(command, args, options)`       | remote   | process:exec   | P1     |
| `child.stdout` / `child.stderr`              | remote   | process:exec   | P1     |
| `child.stdin.write(data)`                    | remote   | process:exec   | P1     |
| `child.kill(signal?)`                        | remote   | process:exec   | P1     |
| `child.onExit(listener)`                     | remote   | process:exec   | P1     |
| `localProcess.spawn(command, args, options)` | local    | process:local  | P2     |
| `net.fetch(request)`                         | both     | net            | P2     |
| `net.connect(target)`                        | both     | net            | P2     |

`process.spawn`
の標準出力と標準エラーは、チャンク単位のストリームとして届きます。プロセスの終了コードは
`onExit` で受け取ります。

## 10. コマンドとキーバインド

| API                                     | 実行場所 | ケイパビリティ | 優先度 |
| :-------------------------------------- | :------- | :------------- | :----- |
| `commands.registerCommand(id, handler)` | local    | なし           | P0     |
| `commands.execute(id, args?)`           | both     | なし           | P0     |
| `commands.getCommands()`                | local    | なし           | P1     |
| `keymap.bind(keys, target, options?)`   | local    | なし           | P0     |
| `keymap.unbind({ keys })`               | local    | なし           | P0     |
| `keymap.setContext({ key, value })`     | local    | なし           | P0     |
| `keymap.registerMode(definition)`       | local    | なし           | P2     |
| `keymap.sequenceTimeout`                | local    | なし           | P2     |

コマンド ID は `<pluginId>.<name>` の形にします。組み込みのコマンドは `vy.`
で始めます。`keymap.bind` の第2引数はコマンド ID か関数です。`options.when`
にはコンテキスト式を書きます。

Phase 3 で入れたペイン操作（`vy.pane.split_right`、`vy.pane.focus_left`、
`vy.pane.close` など）は、キーマップと同じ ID で `commands.getCommands` に現れ、
`commands.execute` からも実行できます。戻り値はまだ `null` です。
`vy.ui.command_palette` はクライアントのコマンドパレットを開きます。

`:` の Ex コマンドはクライアントが別名からコマンド ID に解決し、パレットと同じ
実行層へ渡します。`commands.execute` はコマンド ID だけを受け付け、Ex 名は解決
しません。文法、範囲、`!`、補完は [`ex-commands.md`](./ex-commands.md)
にあります。

```typescript
keymap.bind("normal", "space f f", "vy.find_files", { when: "editorFocus" });
```

## 11. 設定

| API                                 | 実行場所 | ケイパビリティ | 優先度 |
| :---------------------------------- | :------- | :------------- | :----- |
| `config.get({ key })`               | both     | なし           | P0     |
| `config.set(key, value, options?)`  | local    | なし           | P0     |
| `config.inspect({ key })`           | local    | なし           | P1     |
| `config.onDidChange(key, listener)` | local    | なし           | P0     |
| `config.define(schema)`             | local    | なし           | P1     |

設定は4層で解決します。下の層ほど優先します。

1. 組み込みの既定値
2. `~/.config/vy/settings.json`
3. `<workspace>/.vy/settings.json`
4. `init.ts` から呼んだ `config.set`

`init.ts`
からの設定はメモリ上だけに反映し、ファイルには書きません。`config.inspect`
は各層の値と、どれが有効かを返します。`config.define` はプラグインの設定項目を
JSON Schema として登録し、設定画面の生成と検証に使います。

## 12. 通知、進捗、ログ

| API                                    | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------- | :------- | :------------- | :----- |
| `notifications.show(message)`          | local    | なし           | P0     |
| `notifications.onDidClose(listener)`   | local    | なし           | P2     |
| `progress.withProgress(options, task)` | local    | なし           | P1     |
| `log.createChannel(name)`              | local    | なし           | P0     |
| `channel.info(message, data?)`         | local    | なし           | P0     |
| `channel.warn(message, data?)`         | local    | なし           | P0     |
| `channel.error(message, data?)`        | local    | なし           | P0     |

`notifications.show` は level、message、actions を受け取る設計です。Phase 2 では
`message` だけを受け付け、`level` と `actions` は Phase 3
以降です。`progress.withProgress` は `report` と `CancellationToken`
を渡します。

## 13. ペインとビュー

Vy
は固定のサイドバーやパネルを作らず、画面に出るものはすべてペインとして扱います。ウィンドウ、ペイン、ビューのモデルと、`windows.*`、`panes.*`、`views.*`
の API は [`panes.md`](./panes.md) にまとめました。

ここには、`ui` ビューを構成する API と、ビューの中の一部を差し替える API
を残します。

### 13.1 ネイティブ UI

`ui` ビューの木を組み立てる API
です。ほとんどはクライアントで完結します。`ui.setState`
だけは、復元のためにコアへ値を保存します。生のキー入力は扱わず、操作はコマンド
ID で指定します。キー入力との対応は [`ui.md`](./ui.md) にまとめました。

| API                                         | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------------ | :------- | :------------- | :----- |
| `ui.column/row/stack/scroll/tabs`           | local    | なし           | P1     |
| `ui.text/markdown/codeBlock/label`          | local    | なし           | P1     |
| `ui.list/tree/table`                        | local    | なし           | P1     |
| `ui.textInput/checkbox/select`              | local    | なし           | P1     |
| `ui.button/iconButton/link/menu`            | local    | なし           | P1     |
| `ui.image/progress/spinner/badge/separator` | local    | なし           | P1     |
| `ui.update(widgetId, props)`                | local    | なし           | P1     |
| `ui.setState(paneId, state)`                | both     | なし           | P1     |
| `ui.focus(widgetId)`                        | local    | なし           | P2     |
| `ui.scrollTo(widgetId, position)`           | local    | なし           | P2     |

`ui.update`
は、木全体を作り直さずに1つのノードを更新します。ストリーミングの表示に使います。`ui.setState`
は、ペインの復元に使う値をコアへ保存します。

### 13.2 スロット

| API                                                | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------------------------- | :------- | :------------- | :----- |
| `ui.statusline.createItem(options)`                | local    | なし           | P1     |
| `ui.picker.registerSource(source)`                 | local    | なし           | P1     |
| `ui.hover.registerProvider(languageId, provider)`  | local    | なし           | P2     |
| `ui.gutter.registerProvider(languageId, provider)` | local    | なし           | P2     |

`registerSource` と `registerProvider` は、ホストから JS
の関数を呼ぶ経路ができるまで未提供です（Phase 3）。

スロットに渡せるのは、テキスト、アイコン名、色トークン、コマンド
ID、進捗の値です。任意のレイアウトは渡せません。自由な見た目が必要な場合は `ui`
ビューのウィジェット木を使います。Webview は、Web
の文書を表示するときだけ使います。

#### ステータスライン API

ステータスラインはウィンドウ最下部に配置されるグローバルスロットです（[ADR 0006](./adr/0006-user-controlled-ui-and-slots.md)）。

```typescript
export interface StatuslineItemOptions {
  id?: string;
  alignment: StatuslineAlignment;
  priority?: number; // 数値が大きいほど外側（左ならより左、右ならより右）に配置
  text: string;
  color?: ThemeColor; // テーマのトークン名
  command?: string; // クリック時に実行されるコマンド ID
}

export interface StatuslineItem extends Disposable {
  text: string;
  color?: ThemeColor;
  command?: string;
  show(): void;
  hide(): void;
  dispose(): void;
}
```

既定では標準 Vim
相当の項目（モード、ファイル名、変更状態、カーソル位置、進捗）が登録されます。利用者は
`init.ts` やプラグインから項目を個別に追加・更新・破棄（`dispose()`）できます。

## 14. テーマ

| API                           | 実行場所 | ケイパビリティ | 優先度 |
| :---------------------------- | :------- | :------------- | :----- |
| `theme.register(definition)`  | local    | なし           | P1     |
| `theme.token(name)`           | local    | なし           | P1     |
| `theme.onDidChange(listener)` | local    | なし           | P1     |

テーマは色トークンの集合として定義します。プラグインはトークンを追加できますが、既存のトークンの意味は変えません。

## 15. Webview

| API                                  | 実行場所 | ケイパビリティ | 優先度 |
| :----------------------------------- | :------- | :------------- | :----- |
| `webview.createView(options)`        | local    | なし           | P3     |
| `view.postMessage(message)`          | local    | なし           | P3     |
| `view.onDidReceiveMessage(listener)` | local    | なし           | P3     |
| `view.asWebviewUri(uri)`             | local    | なし           | P3     |
| `view.state`                         | local    | なし           | P3     |
| `view.reload()`                      | local    | なし           | P3     |
| `view.dispose()`                     | local    | なし           | P3     |

`webview.createView` はビューのハンドルを返します。ペインに載せるときは
`panes.open({ view: handle })` を使います。Webview と QuickJS の間は JSON
で直列化できる値だけを渡します。`options` には
html、scripts、csp、ローカル資源のルートを指定します。状態の復元には `state`
を使います。

Webview は最終手段です。Notebook の出力、HTML
のプレビュー、外部の埋め込みなど、Web
の文書そのものを表示するときに限ります。操作を伴う UI は `ui`
ビューで作ります。Webview
の中のキー入力はキーマップの外にあり、モード、カウント、IME
の前編集を共有できないためです。外側のフォーカス、`Esc`
での離脱、テーマの配信はホストが持ちます。詳しくは [`ui.md`](./ui.md)
の「Webview の位置づけ」にあります。

## 16. 言語機能

| API                                                          | 実行場所 | ケイパビリティ | 優先度 |
| :----------------------------------------------------------- | :------- | :------------- | :----- |
| `languages.register(definition)`                             | both     | なし           | P1     |
| `languages.registerSnippet(scope, snippets)`                 | local    | なし           | P2     |
| `languages.registerFormatter(languageId, formatter)`         | remote   | なし           | P2     |
| `languages.registerCompletionProvider(languageId, provider)` | both     | なし           | P2     |
| `lsp.registerServer(id, options)`                            | remote   | process:exec   | P2     |
| `lsp.getClient(id)`                                          | both     | なし           | P2     |

`languages.register`
は拡張子、コメント記号、インデント幅などの宣言的な情報だけを扱います。構文解析はコアが持ちます。

## 17. 実行環境とユーティリティ

| API                                  | 実行場所 | ケイパビリティ  | 優先度 |
| :----------------------------------- | :------- | :-------------- | :----- |
| `remote.isRemote`                    | local    | なし            | P0     |
| `remote.name`                        | local    | なし            | P0     |
| `remote.os`                          | local    | なし            | P1     |
| `remote.pathSeparator`               | local    | なし            | P1     |
| `clipboard.readText()`               | local    | clipboard:read  | P2     |
| `clipboard.writeText(text)`          | local    | clipboard:write | P2     |
| `env.openExternal(uri)`              | local    | openExternal    | P2     |
| `cancellation.createSource()`        | both     | なし            | P0     |
| `events.on(topic, filter, listener)` | local    | なし            | P2     |

`editor.remote` は廃止しました。リモートの有無で API を分けず、`fs` や `process`
が接続先を見ます。

## 18. バックエンド API

`backend.ts`
は接続先のコアで動きます。重い処理、ファイル監視、外部プロセスの結合に使います
（[ADR 0011](./adr/0011-backend-plugins-on-core.md)）。フロントとバックエンドで
コアの名前空間を共有し、`backend` はチャネルとライフサイクルだけを持ちます。

### 18.1 ライフサイクルとチャネル

| API                                         | 呼べる側   | 優先度 |
| :------------------------------------------ | :--------- | :----- |
| `activate(ctx)`、`deactivate()`             | backend    | P1     |
| `ctx.pluginId`、`ctx.version`               | backend    | P1     |
| `ctx.capabilities.has()`                    | backend    | P1     |
| `ctx.log`、`ctx.subscriptions`              | backend    | P1     |
| `ctx.platform`                              | backend    | P1     |
| `backend.connect(channel)`                  | frontend   | P1     |
| `backend.registerHandler(channel, handler)` | backend    | P1     |
| `channel.postMessage(message)`              | both       | P1     |
| `channel.onMessage(listener)`               | both       | P1     |
| `channel.request(message, options?)`        | frontend   | P1     |
| `channel.onDidClose(listener)`              | frontend   | P1     |
| `channel.dispose()`                         | both       | P1     |
| `req.postMessage(message)`、`req.signal`    | backend    | P1     |

チャネルは文字列で識別します。同じ名前のチャネルを2つのプラグインが登録した場合は、後の登録をエラーにします。
`request` は handler の戻り値を返します。ストリーミングは `req.postMessage` を使います。
チャネルごとに順序を保証し、1つのメッセージの上限を超えた場合は
`ResourceLimit` で閉じます。

### 18.2 コアの名前空間

| 名前空間       | バックエンドで使える API                                                                 | 優先度 |
| :------------- | :--------------------------------------------------------------------------------------- | :----- |
| `workspace`    | `folders`、`rootUri`、`name`、`textDocuments`、`openTextDocument`、`findFiles`、`grep`、`createFileSystemWatcher`、`onDidChangeTextDocument` | P1     |
| 文書           | `uri`、`version`、`languageId`、`lineCount`、`getText`、`lineAt`、`isDirty`、`eol`      | P1     |
| `fs`           | `readFile`、`readTextFile`、`writeFile`、`readDir`、`stat`、`exists`、`createDir`、`remove`、`rename`、`copy` | P1     |
| `process`      | `spawn`、`exec`、標準出力 / 標準エラー、stdin、kill、終了                               | P1     |
| `config`       | `get`、`onDidChange`                                                                     | P1     |
| `log`          | `createChannel`                                                                          | P1     |
| `cancellation` | `createSource`                                                                           | P1     |
| `workspace`    | `applyEdit`                                                                              | P2     |
| `diagnostics`  | `set`、`clear`                                                                           | P2     |
| `net`          | `fetch`                                                                                  | P2     |
| `languages`    | `registerFormatter`、`registerCompletionProvider`                                        | P2     |
| `lsp`          | `getClient`                                                                              | P2     |
| `fs`           | `watch`                                                                                  | P2     |

`backend.ts` には次の API を出しません。

- UI: `ui`、`views`、`panes`、`windows`、`webview`、`notifications`、`progress`、`keymap`、
  `commands.registerCommand`
- クライアント状態: `editor.activeDocument`、`editor.mode`、`editor.selections`、
  `editor.setMode`、`editor.open`、`editor.reveal`
- クライアント側の資源: `localFs`、`localProcess`、`clipboard`、`openExternal`、
  `workspace.state`
- 設定の書き込み: `config.set`、`config.define`
- `remote.*`: バックエンドは常にコア側なので、接続の有無ではなく `ctx.platform` を使う

バックエンドの API は暗黙のアクティブ文書を使いません。URI と範囲を明示的に受け取ります。
文書の変更イベントは差分で送り、初回だけ `openTextDocument(uri)` でスナップショットを取ります。

## 19. 型

| 型                  | 内容                                                                                |
| :------------------ | :---------------------------------------------------------------------------------- |
| `Uri`               | `file:` または `vy:` のスキームを持つ。`vy:` は未保存の文書、差分、プレビューに使う |
| `Position`          | 0 始まりの line と、UTF-16 単位の character                                         |
| `Range`             | start と end の `Position`                                                          |
| `Selection`         | anchor と head の `Position`                                                        |
| `TextEdit`          | `Range` と新しい文字列                                                              |
| `WorkspaceEdit`     | 複数ファイルの `TextEdit` の集合                                                    |
| `Disposable`        | `dispose()` を持つ                                                                  |
| `CancellationToken` | `isCancelled` と `onCancelled` を持つ                                               |
| `ThemeColor`        | 色トークンの名前                                                                    |
| `PaneId`            | ペインの識別子                                                                      |
| `WindowId`          | ウィンドウ（タブ）の識別子                                                          |
| `ViewType`          | ビューの種類の名前                                                                  |
| `ViewHandle`        | ビューのインスタンス。ペインに載せる                                                |
| `UINode`            | `ui` ビューのウィジェット木のノード。`id`、種類、props、子を持つ                    |
| `WidgetId`          | ウィジェットの識別子。フォーカスと更新の単位                                        |
| `WidgetAction`      | ウィジェットが組み込みコマンドに対応づけるアクション名                              |
| `LayoutNode`        | `Split` か `Leaf`                                                                   |
| `PanePlacement`     | 対象のペイン、方向、比率                                                            |
| `FloatingRect`      | フローティングの位置と大きさ                                                        |

`ThemeColor`（`default`、`muted`、`primary`、`info`、`success`、`warning`、`danger`、`error`
など）と、ウィジェットの語彙（`Gap` の `sm`/`md`/`lg`、`Font` の
`mono`、`ButtonVariant` の `primary`/`danger`）は列挙です。未知の値は
`InvalidParams`
で失敗し、描画側が既定の見た目に化けることはありません。これらの語彙は閉じており、追加はクライアントの変更として行います（[ADR 0007](./adr/0007-rust-typed-plugin-api.md)
§6）。

## 20. エラー

API の失敗は例外で返します。例外は `code` を持ちます。

| code                 | 条件                                           |
| :------------------- | :--------------------------------------------- |
| `InvalidRequest`     | メソッド名が不正                               |
| `InvalidParams`      | 引数の形か型が不正                             |
| `CapabilityDenied`   | 宣言のないケイパビリティを使おうとした         |
| `InvalidState`       | 対象の文書やペインがすでに閉じている           |
| `Cancelled`          | 呼び出しが取り消された                         |
| `Timeout`            | 制限時間を超えた                               |
| `RemoteDisconnected` | 接続が切れた                                   |
| `ResourceLimit`      | プラグインのメモリまたは実行時間の上限を超えた |

`RemoteDisconnected`
を受け取ったプラグインは、自動で再接続しません。再同期はコアクライアントが行います。

## 21. Phase 2 の実装状況

Phase 2 では `init.ts` から呼べる範囲を次のとおり実装しました。型は
[`packages/vy-api/index.d.ts`](../packages/vy-api/index.d.ts)（生成）と
[`packages/vy-api/overrides.d.ts`](../packages/vy-api/overrides.d.ts)（糖衣）にあり、引数の形は
§2.1 のとおりです。

| 名前空間                            | 実装済み                                                                                                |
| :---------------------------------- | :------------------------------------------------------------------------------------------------------ |
| `commands`                          | `registerCommand`、`execute`、`getCommands`                                                             |
| `keymap`                            | `bind`（`options.when` と `options.mode`）、`unbind`、`setContext`                                      |
| `config`                            | `get`、`set`（オブジェクト形式と `(key, value)` 形式）、`inspect`、`onDidChange`                        |
| `editor`                            | `activeDocument`、`documents`、`mode`、`setMode`、`selections`、`setSelections`、`edit`、`undo`、`redo` |
| `document`（`Document` のメソッド） | `getText`、`lineAt`、`positionAt`、`offsetAt`、`fullRange`                                              |
| `workspace`                         | `rootUri`、`name`、`trusted`、`folders`、`textDocuments`                                                |
| `notifications`                     | `show`                                                                                                  |
| `log`                               | `createChannel`（`info`、`warn`、`error`。`~/.cache/vy/logs/init.log` に書く）                          |
| `remote`                            | `isRemote`、`name`、`os`、`pathSeparator`                                                               |
| `context`                           | `pluginId`、`version`、`capabilities.has`、`log`                                                        |

スロットは `ui.statusline`、`ui.gutter`、`ui.hover`、`ui.picker.show`
とウィジェット木のビルダーだけを提供します。プロバイダの登録（`ui.picker.registerSource`、`ui.gutter.registerProvider`、`ui.hover.registerProvider`）は、ホストから
JS の関数を呼ぶ経路ができるまで API から外しました（Phase 3）。

先送りしたもの:

- `workspace.state`、`fs`、`process`、`net`。`workspace.state` と `fs` /
  `process` / `net` は Phase 3 以降です。
- JS の `editor.open` と `document.save` API はまだありません。クライアントの
  コマンド `vy.editor.open`（`:e`）と `vy.document.save`（`:w`）は実装済みです。
- `document.onDidChangeContent`、`editor.onDidChangeSelection` などのイベント。
  発火するには UI から JS へのイベント経路がもう1本必要です（Phase 3）。
- `config.define`、`ui` ビュー、スロット、テーマは Phase 3。Webview は Phase 8
  です。
- `editor.undo()` と `editor.redo()` は要求を送るところまでです。結果の適用は
  コアの応答で行うため、返り値は `void` です。
- `commands.execute` の返り値は `null` です。JS 側の戻り値を返すのは Phase 3
  です。
- `editor.edit` は1回の呼び出しを1トランザクションにしますが、複数カーソルの
  選択はまだ扱いません。
- `notifications.show` の `level` と `actions`、`notifications.onDidClose` は
  Phase 3 以降です。今は `message` だけを受け付けます。

`init.ts` は常に信頼し、ケイパビリティの承認を求めません。外部プラグインの
マニフェストと承認 UI は Phase 3 です。

## 22. 未決定事項

- 補完プロバイダをフロントとリモートのどちらで実行するか。候補入力の遅延を抑えるならフロント、言語サーバーに近い方がよければリモートです。
- `secrets` の保存先を OS のキーリングにするか、暗号化したファイルにするか。
- Webview に渡せる値の範囲を JSON に限るか、構造化複製まで広げるか。
- プラグインが独自の言語サーバーを登録できるようにするか、コアの設定に限定するか。
- `events.on` のトピック名の一覧と、購読の上限。
- `ui`
  ビューの木の上限、読み取り専用テキストのモーション、テキスト入力の既定モードなどは
  [`ui.md`](./ui.md) にまとめました。
- レイアウトの未決定事項は [`panes.md`](./panes.md) にまとめました。
- CRLF と EOL の扱い。`document.eol` と `document.setEol` をいつ入れるか、
  改行コードをクライアントとコアのどちらで保持するか。Phase 2 では扱いません。
