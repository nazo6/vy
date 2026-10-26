---
id: "0014"
title: "API バインディングの生成を CLI に分離する"
type: "adr"
status: "superseded"
date: 2026-10-24
supersedes: []
superseded_by: "0025"
---

# ADR 0014: API バインディングの生成を CLI に分離する

> この決定は [ADR 0025](../0025-plugin-api-declaration-and-codegen.md) に引き継ぎました。
> 実装と検証の記録は [worklog](../../worklogs/2026-10-24-plugin-api-pipeline.md) にあります。

- 状態: 採用（出力レイアウトは [0015](./0015-api-output-layout.md)、
  `CommandId` と `overrides.d.ts` は
  [0016](./0016-command-types-and-ts-shim.md) で置き換え）
- 日付: 2026-10-24

## 文脈

ADR 0009 は TypeScript の生成を `codegen::export` に集めましたが、その呼び出しは
`vy-app` の `export_bindings_plugin_api` テストに残していました。`CommandId.ts` と
`SettingsKey.ts` も `vy-app` のテストが書いていました。

この形には次の問題があります。

1. 生成のために `vy-app` をビルドする。`vy-app` は GPUI に依存するため、生成だけを
   行うときもクライアント全体の依存をコンパイルすることになる。
2. テストがリポジトリへ書き込む。テストの実行順や並列度によって、生成結果が
   別のテストの入力になる。
3. Phase 6 で `backend.ts` 用のコア限定バインディングを追加すると、クライアント用と
   バックエンド用の2つの出力を同じテストから制御することになる。
4. バックエンド用の生成に `vy-app` が不要でも、テストの置き場所の都合で
   `vy-app` に引きずられる。

## 決定ドライバ

1. 生成は実行時コードから独立していること。
2. クライアント用とバックエンド用を同じ仕組みで生成できること。
3. `vy-app` と GPUI を生成の依存に含めないこと。
4. テストは生成物を書き換えないこと。
5. CI で生成物の差分を検査できること。

## 検討した選択肢

| 選択肢                               | 内容                                         | 評価                                                           |
| :----------------------------------- | :------------------------------------------- | :------------------------------------------------------------- |
| A. テストのまま                      | `vy-app` のテストで `codegen::export` を呼ぶ | 現状維持。GPUI 依存とテストの書き込みが残る                    |
| B. `vy-app` のサブコマンド           | `vy-app --export-bindings` を追加する        | 起動経路は増えるが、生成がクライアントに残る                   |
| C. `cargo xtask`                     | ワークスペースのタスク crate に生成を置く    | 一般的だが、`vy-app` のメタデータを参照すると GPUI 依存が残る  |
| D. `vy-api-codegen` クレート（採用） | API クレートだけに依存する CLI を追加する    | 生成の依存が軽く、クライアントとバックエンドを同じ入口で扱える |

## 決定

### 1. `vy-api-codegen` を追加する

`crates/vy-api-codegen` をワークスペースに追加し、バイナリを生成の唯一の入口に
します。依存は `vy-api-schema` と `vy-client-api` だけで、`vy-app`、`vy-server`、
GPUI には依存しません。

```sh
cargo run -p vy-api-codegen -- client
cargo run -p vy-api-codegen -- client --check
```

`client` は現在の `packages/vy-api`（型は `types/`）を書きます。
`--check` は一時ディレクトリに生成し、コミット済みのファイルと比較するだけで、
リポジトリを書き換えません。

### 2. クライアント専用のメタデータを `vy-client-api` に移す

`CommandId.ts` と `SettingsKey.ts` の生成に必要な情報を `vy-app` から
`vy-client-api` に移します。

- `vy_client_api::command`: `BuiltinCommand`、`PaneCommand`、`DocumentCommand`、
  `UiCommand`、`ViewCommand`
- `vy_client_api::settings`: `SettingDecl`、`SETTINGS`、`defaults()`、
  `default_font_family()`

`vy-app` はこれらを再エクスポートし、`Config` の既定値も同じ `defaults()` を
使います。コマンド ID と設定キーの正が API クレートに移り、生成と実行時で
同じ定義を参照します。

### 3. 出力先を引数にする

`vy-api-schema::codegen` に `Output` と `export_to` を追加し、出力先を
`types_dir` と `api_dir` で受け取るようにします。`--check` はこの入口に一時
ディレクトリを渡します。将来のバックエンド用生成も、`vy-host-api` の宣言と
`Output` を渡す形で追加できます。

### 4. テストから生成を外す

`vy-app` の `export_bindings_*` テストを削除します。宣言の完全性を検査する
テストは残しますが、ファイルは書きません。`vy-app` の `ts-rs` 依存も外します。

CI では次を実行し、生成物の差分を検査します。

```sh
cargo run -p vy-api-codegen -- client --check
```

## 帰結

### 良い面

- 生成に GPUI と `vy-app` のビルドが不要になる。
- テストがリポジトリを書き換えなくなる。
- クライアント用とバックエンド用の出力を同じ CLI で扱える。
- `CommandId` と `SettingsKey` の正が API クレートに集まる。

### 悪い面

- 生成手順が `cargo test` から独立したコマンドになる。
- `vy-client-api` が設定の既定値とコマンド ID を持つ。
- `--check` は生成ファイルだけを比較するため、生成対象から外れた古いファイルは
  検出しない。

### 緩和策

- 生成物の冒頭に再生成コマンドを書く。
- `docs/development.md` と各パッケージの README に CLI を書く。
- CI で `--check` と `git diff --exit-code packages` を併用する。
- 出力先を `Output` にまとめ、バックエンド用のテストを追加しやすくする。

## 見直しの条件

1. バックエンド用の型をクライアントの `types/` と分ける必要が出たとき。
2. 生成対象のファイルが増え、`--check` の比較だけでは不足するとき。
3. 生成をビルドスクリプトやパッケージ管理ツールへ統合するとき。

## 関連する未決定事項

- `backend.d.ts` の出力先と、コア限定の型ファイルをクライアントと共有するか。
- `--check` で生成対象外の古いファイルも検出するか。
- `vy-api/backend` のパッケージ export と、グローバル `vy` の型の扱い。
