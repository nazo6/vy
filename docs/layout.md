# ファイルとフォルダの構成

Vy
が読み書きする場所と、リポジトリの構成を決めます。設定、プラグイン、テーマ、スニペットの置き場所を先に固定すると、API
の引数と解決順序も決まります。

## 1. ユーザー設定ディレクトリ

ユーザーが編集するファイルを置きます。

```text
~/.config/vy/
  init.ts
  settings.json
  connections.json
  plugins/
    <publisher>.<name>/
      plugin.json
      dist/
        frontend.js
        backend.js
      README.md
  themes/
    <name>.json
  snippets/
    <languageId>.json
  languages/
    <languageId>.json
  queries/
    <languageId>/
      highlights.scm
      injections.scm
  trusted.json
  workspace-state/
    <workspace-hash>.json
```

`init.ts` は設定用のスクリプトです。`settings.json` は宣言的な設定で、UI
から変更した値もここに書きます。両方を用意するのは、キーバインドのように条件式が欲しい設定と、フォントサイズのように値だけで足りる設定があるためです。`init.ts`
は `settings.json` より優先しますが、ファイルには書きません。

`plugins/`
の直下にはインストール済みのプラグインだけを置きます。開発中のプラグインは、`init.ts`
から `plugins.load(path)` で読み込みます。

`trusted.json` には、承認したケイパビリティをプラグイン ID
とバージョンの組で記録します。プラグインを更新してケイパビリティが増えた場合は、その差分だけを再承認します。

`connections.json`
には、リモート接続のプロファイルを置きます。`host`、`user`、`port`、`workspace`、
`home`、`serverPath` を持ち、`vy-app --connect <name>` で選びます
（[ADR 0010](./adr/0010-remote-connection.md)）。接続先の `vy-server`
は導入済みのものを使い、自動では転送しません。

OS ごとの既定の場所は次のとおりです。

| OS      | 設定                               | キャッシュ                | データ                             |
| :------ | :--------------------------------- | :------------------------ | :--------------------------------- |
| Linux   | `~/.config/vy`                     | `~/.cache/vy`             | `~/.local/share/vy`                |
| macOS   | `~/Library/Application Support/vy` | `~/Library/Caches/vy`     | `~/Library/Application Support/vy` |
| Windows | `%APPDATA%\vy`                     | `%LOCALAPPDATA%\vy\cache` | `%APPDATA%\vy`                     |

## 2. キャッシュとデータ

ユーザーが直接編集しないファイルを置きます。

```text
<キャッシュ>/
  bundles/
    <pluginId>/<version>.js
  queries/
    <languageId>/<ハッシュ>.bin
  logs/
    vy.log
    init.log
    plugins/<pluginId>.log

<データ>/
  history.json
  session.json
  state/
    <pluginId>.json
  workspace-state/
    <workspace-hash>.json
```

`bundles/` は、ホストがソースを変換した結果をキャッシュする場所です。配布用の
バンドルはプラグインの `dist/` に置き、ここには置きません。`queries/`
には、コンパイルした tree-sitter
のクエリを置きます。どちらも削除しても再生成できます。

`session.json`
には、開いていたウィンドウ、接続先、ウィンドウの大きさを保存します。ペインのレイアウト木はコアが
`workspace-state/` に保存するため、接続先が変わっても同じ形で復元できます。保存の単位は
[`panes.md`](./panes.md) にまとめました。

## 3. プロジェクト

プロジェクトの直下に `.vy/` を置きます。

```text
<workspace>/
  .vy/
    settings.json
    tasks.json
```

`settings.json`
はそのプロジェクトだけに適用する設定です。チームで共有する前提にし、実行されるコードは置きません。プロジェクト側の
TypeScript
は採用しません。未信頼のリポジトリを開いた時点でコードが動くことを避けるためです。

`tasks.json` はビルドやテストのコマンド定義です。`vy.task` から実行します。

## 4. リポジトリ

```text
crates/
  vy-app/            GPUI のクライアント。ウィンドウ、入力、ウィジェット描画、スロット描画、Webview ホスト
  vy-core/           文書、バッファ、ワークスペース、LSP、Git、検索、設定の解決
  vy-server/         ヘッドレスのバイナリ。RPC の受け口とセッション管理
  vy-proto/          RPC の型とバージョン
  vy-js/             QuickJS のホスト。グローバル `vy` の束縛、相対モジュールの
                     解決、イベントループ
                     lib.rs / vm.rs / transpile.rs / api.rs / shim.ts
                     shim.js は shim.ts から生成する
  vy-capabilities/   ケイパビリティの定義、承認の記録、検査

packages/
  vy-api/            プラグイン API の型パッケージ。ambient な `vy` と
                     `Vy.CommandMap` を宣言する。types/ と api-base.d.ts は
                     Rust から、shim.d.ts は vy-js の shim.ts から生成する
  vy-cli/            任意の補助ツール。verify、doctor、install を持ち、
                     ビルドは持たない
  create-vy-plugin/  雛形の生成

assets/
  themes/
  languages/
  queries/
  grammars/

builtin/
  plugins/           ビルトインの TS プラグイン
  keymaps/           既定のキーバインド（default.json）

examples/
  init.ts            設定スクリプトの例
  plugins/
  workspaces/

docs/
  adr/
```

`vy-app` だけが GPUI に依存します。`vy-core` と `vy-server`
は描画に依存しません。この境界は CI で検査します。

ビルトインのプラグインは、ユーザーのプラグインと同じ仕組みで読み込みます。API
の不足を早く見つけるためです。ただし、承認は省略し、起動の優先度を上げます。

## 5. プラグインのパッケージ

```text
<plugin>/
  plugin.json
  frontend.ts
  backend.ts
  env.d.ts
  package.json
  tsconfig.json
  README.md
  dist/
    frontend.js
    backend.js
```

`plugin.json` は Vy が読むマニフェストです。`package.json`
はビルド時の依存解決にだけ使います。`env.d.ts` には
`import type {} from "vy-api";` を置き、`vy` の型を有効にします。

```json
{
  "id": "example.hello",
  "name": "Hello",
  "version": "0.1.0",
  "engines": { "vy": ">=0.1.0" },
  "main": "dist/frontend.js",
  "backend": "dist/backend.js",
  "permissions": ["fs:read:${workspace}/**"],
  "activationEvents": ["onCommand:example.hello.greet"],
  "contributes": {
    "commands": [{ "id": "example.hello.greet", "title": "Hello" }]
  }
}
```

バンドル形態で配布するときは `frontend.ts` と `backend.ts` をバンドルし、
`dist/` だけを含めます。利用者の環境で `node_modules` を解決しません。ソース
形態はソースのまま配布できますが、外部依存は使えません。

プラグインには2つの形態があります（[ADR 0020](./adr/0020-plugin-source-and-bundles.md)）。

- **ソース形態**: `main` が `frontend.ts` を指し、相対 import だけで書きます。
  外部依存は使えません。ビルドなしで動きます。
- **バンドル形態**: `main` が `dist/frontend.js` を指し、外部依存をすべて
  同梱します。

どちらも `vy` グローバルを使い、`vy-api` からの import は書きません
（[ADR 0019](./adr/0019-global-vy-api.md)）。型は `env.d.ts` などで
`import type {} from "vy-api"` を1回読み込みます。

バンドルは esbuild、tsup、rolldown-vite など一般の JS ツールチェーンで作ります。
Vy はバンドラも `vy plugin build` も提供しません。

`backend.ts` はリモートへ1ファイルで送るため、常にバンドルします。

## 6. 解決順序

同じ名前の資源が複数の場所にある場合は、次の順で先に見つかったものを使います。

| 資源       | 順序                                                                              |
| :--------- | :-------------------------------------------------------------------------------- |
| 設定       | 既定値、ユーザーの `settings.json`、プロジェクトの `.vy/settings.json`、`init.ts` |
| テーマ     | ビルトイン、ユーザーの `themes/`                                                  |
| スニペット | ユーザーの `snippets/`、ビルトイン                                                |
| 言語定義   | ビルトイン、ユーザーの `languages/`                                               |
| クエリ     | ユーザーの `queries/`、ビルトイン                                                 |

テーマはユーザー側を優先し、言語定義とクエリはビルトインを優先します。言語定義を利用者が書き換えると、構文解析の前提が崩れるためです。

## 7. 命名規則

- プラグイン ID は `<publisher>.<name>` の形にし、英小文字とハイフンを使います。
- コマンド ID は `<pluginId>.<command>` の形にし、各セグメントには英小文字、
  数字、`-`、`_` を使います。組み込みは `vy.` で始めます。
- 設定キーは `<pluginId>.<key>` の形にします。組み込みは
  `ui.`、`editor.`、`lsp.` などの名前空間を使います。
- 言語 ID は VS Code と同じ名前を使います。`rust`、`typescript`、`markdown`
  などです。

## 8. 未決定事項

- プラグインのインストール先を、設定ディレクトリと分けるか。書き込み権限のない環境では、データディレクトリに置く必要があります。
- プロジェクトごとのプラグイン指定を `.vy/settings.json`
  に書くか、別のファイルにするか。
- `queries/`
  の上書きを許可する範囲。ハイライトだけ許可し、インデントやテキストオブジェクトは許可しない案があります。
