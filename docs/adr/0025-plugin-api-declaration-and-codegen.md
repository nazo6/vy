---
id: "0025"
title: "プラグイン API の宣言・型・生成を名前空間 trait と Specta に集約する"
type: "adr"
status: "implemented"
category: "architecture"
date: 2026-10-27
supersedes:
  ["0007", "0008", "0009", "0012", "0013", "0014", "0015", "0016", "0023"]
superseded_by: null
tags: ["api", "codegen", "plugin"]
---

# ADR 0025: プラグイン API の宣言・型・生成を名前空間 trait と Specta に集約する

- 状態: 採用（実装済み）
- 日付: 2026-10-27
- 置き換え: [0007](./superseded/0007-rust-typed-plugin-api.md)（型は Rust が正）、
  [0008](./superseded/0008-declare-plugin-api-with-macros.md)（宣言マクロ）、
  [0009](./superseded/0009-api-types-live-with-handlers.md)（型の置き場所）、
  [0012](./superseded/0012-shared-host-api.md)（共有ホスト API）、
  [0013](./superseded/0013-host-api-traits.md)（名前空間 trait）、
  [0014](./superseded/0014-api-codegen-cli.md)（codegen CLI）、
  [0015](./superseded/0015-api-output-layout.md)（出力レイアウト）、
  [0016](./superseded/0016-command-types-and-ts-shim.md)（コマンド型と shim）、
  [0023](./superseded/0023-specta-api-types.md)（Specta への移行）
- 実装の記録: [`2026-10-24-plugin-api-pipeline.md`](../worklogs/2026-10-24-plugin-api-pipeline.md)

## 文脈

プラグイン境界の型と TypeScript 生成は、Phase 2 から Phase 5 にかけて段階的に作り直されました。当初は shim、手書きの `index.d.ts`、文字列 match のディスパッチという 3 つの独立した記述があり、API を 1 つ足すたびに 3 箇所を同期する必要がありました。不正な値が既定値に化ける経路も残っていました。

その後、スキーマ crate、宣言マクロ、名前空間 trait、codegen CLI、出力レイアウト、コマンド型と shim の生成、Specta への移行と、決定が 9 本に分かれて積み上がりました。それぞれの判断は正しいものの、**どれが現在有効なのか**が索引から読み取れず、部分的な置き換えの関係も本文を読まないと分からない状態でした。

現在の仕組みは安定して動いており、移行も完了しています。ここで、現在有効な決定だけを 1 本にまとめます。

## 決定ドライバ

1. **単一情報源**: 境界の型・宣言・JSDoc・ワイヤー名が 1 つの定義から出ること。
2. **追加コスト**: メソッドを 1 つ足すときに編集するファイルが 1 つであること。
3. **境界の厳格さ**: 不正な値は既定値に化けず、`InvalidParams` として失敗すること。
4. **生成の再現性**: 生成物が決定的で、CI で差分を検査できること。
5. **分離**: `vy-server` が GPUI に依存せず、クライアント専用 API がコアへ漏れないこと。
6. **要求の低さ**: 生成に `vy-app` と GPUI のビルドを要求しないこと。

## 決定

### 1. 境界の型と宣言は Rust が正

- プラグイン境界に現れる params / reply / イベント payload の型と宣言は Rust で定義し、そこから TypeScript を生成します。TypeScript を手書きして Rust を生成する逆向きの経路は作りません。
- クライアントとコアが共有する API（`fs`、`workspace`、`process`、`document`、`config` の読み取り、`log`、`context`）は `crates/vy-host-api` に置きます。
- クライアント専用 API（`ui`、`views`、`editor`、`keymap`、`commands`、`notifications`、`config` の書き込み）は `crates/vy-client-api` に置きます。
- 境界を越える値型（`Mode`、`ThemeColor`、`WidgetNode`、`PickerItem`、`StatuslineAlignment`）は API クレートの自然なモジュールに置き、`vy-app` の対応するモジュールから再エクスポートします。境界専用の複製は作りません。
- `vy-server` が `vy-app` に依存することはありません。

### 2. 名前空間は trait で宣言し、マクロが dispatch を生成する

- 名前空間ごとに trait を宣言し、`#[vy_api_trait("path")]` を付けます。メソッドは `#[vy_api_method]`、イベントは `#[vy_api_event]` 付きの関連定数で宣言します。
- `host_api!` が facade trait、`dispatch`、`METHODS`、`NAMESPACE`、`EVENTS` を生成します。クライアントは共有 facade にクライアント専用 trait を足し、コアは共有 trait だけを実装します。
- ホストは `ClientHost` / `CoreHost` に各 trait を実装します。メソッドを足すと trait の実装漏れがコンパイルエラーになります。
- イベントは topic 文字列ではなく `HostEvent` の marker で送ります。topic と payload 型がコンパイル時に結びつきます。

### 3. 型はハンドラの隣に置き、Specta で単一ファイルに生成する

- params / reply / イベント payload は、宣言と同じ名前空間モジュールに定義します。JSDoc、serde 属性、派生がメソッド宣言の隣に並びます。
- 型の説明は Rust の doc コメントを正とし、フィールド単位のコメントも JSDoc として出力します。
- 型生成は `specta` と `specta-typescript` を使います。`ts-rs` と、依存型を手で辿る `TypeCollector` は使いません。
- 生成型は `packages/vy-api/types/bindings.ts` の 1 ファイルに集約します。型ごとの個別ファイルと barrel は作りません。

### 4. 生成の入口は `vy-api-codegen` CLI

- `cargo run -p vy-api-codegen -- client` が TypeScript を書きます。`--check` は一時ディレクトリに生成し、コミット済みのファイルと比較するだけで、リポジトリを書き換えません。
- 生成に `vy-app`、`vy-server`、GPUI を含めません。テストはリポジトリへ書き込みません。
- 出力先は `packages/vy-api` にまとめます。生成型は `types/bindings.ts`、wire の宣言は `api-base.d.ts`、手書きの糖衣の型は `shim.ts` から生成する `shim.d.ts`、入口は `index.d.ts` です。JS の実体は `crates/vy-js/src/generated/shim.js` に置きます。
- 開発者の入口は `pnpm generate:api` です。生成物はコミットし、CI で `--check` と差分検査を通します。

### 5. コマンド型を分け、JS 専用メソッドの型は shim から生成する

- 組み込みコマンドの ID と引数は `BuiltinCommandMap`、プラグインが宣言マージで足すコマンドは `CommandMap` です。`registerCommand` は組み込み ID を受け付けません。
- `commands.execute` と `keymap.bind` は conditional type で引数を検査し、未知の ID は `unknown` に落とすフォールバックを持ちます。
- JS 専用のメソッド（コールバック登録、イベント購読、`commands.registerCommand` / `execute`）は `crates/vy-js/src/shim.ts` に実装し、`tsc --declaration` で `shim.d.ts` を生成します。`overrides.d.ts` と `sugar` 属性は使いません。
- 実行時の入口はグローバル `vy` です（[0019](./0019-global-vy-api.md)）。

### 6. 境界では型で検証し、語彙は閉じる

- params はハンドラごとに型付きで復元します。欠落、型違い、範囲外は `InvalidParams` で失敗させ、既定値で埋めません。
- 色、配置、gap、フォント、variant、ログレベルのように描画側が網羅 match する語彙は、閉じた列挙にします。未知の値を受け付ける経路を作りません。
- コマンド ID と設定キーは、既知の集合を union として公開しつつ、`CommandMap` / `SettingsMap` の宣言マージで拡張できる open interface にします。拡張した ID は実行時の登録でも受け付けます。
- `when` は式言語なので列挙にせず、文字列として受け取ってバインド時に解析します。

## 帰結

### 良い面

- メソッドの追加が「trait に宣言し、ホストに実装する」だけで済み、表・match・shim の追記が不要です。
- 型、宣言、JSDoc、ワイヤー名が同じ定義から出るため、同期漏れが起きません。
- 生成が 1 つの CLI と 1 つの出力先にまとまり、`--check` で再現性を検査できます。
- 境界が厳格になり、プラグイン作者の誤りが呼び出し時に表面化します。
- クライアント専用 API がコアへ漏れず、`vy-server` は GPUI に依存しません。

### 悪い面・課題

- 名前空間を増やすときは、trait、facade のリスト、ホストの実装の 3 か所に触れます。メソッド追加は 1 ファイルですが、名前空間追加はそうではありません。
- 生成に TypeScript（`tsc`）と Node が入ります。
- 生成物の差分がレビューでノイズになります。レビューは Rust 側の宣言で行います。
- 実行時に引数の形を検査する仕組みはまだありません。型は Oxc で除去されます。

### 緩和策

- 生成物の冒頭に再生成コマンドを書き、手編集を禁止します。
- `--check` を CI と手元の検証に使い、生成物のずれを早期に見つけます。
- 実行時の引数スキーマは、必要になった時点で `ArgSpec` の検証と統合します。

## 見直しの条件

1. `backend.d.ts` を出し、バックエンド専用の型をクライアントと分けるとき。
2. Specta や `host_api!` の制約で、宣言と実装の一致を保てなくなったとき。
3. ヘッドレスなプラグインホストなど、ハンドラを `vy-app` の外で使う必要が出たとき。
4. 実行時の引数検証を型生成と一体化したくなったとき。

## 関連する決定

- [0019](./0019-global-vy-api.md): プラグイン API はグローバル `vy` に統一する。
- [0018](./0018-typed-rpc.md): クライアント・コア間の RPC は型付き marker で送る。
- [0011](./0011-backend-plugins-on-core.md): バックエンドプラグインはコアで動かす（型の分離は未着手）。

## 関連する未決定事項

- `backend.d.ts` の出力先と、コア限定の型をクライアントと共有するか。
- 同じコマンド ID やイベント topic を複数のプラグインが拡張したときの規則。
- 実行時の引数スキーマを型生成から導くか、手書きの `ArgSpec` を続けるか。
