# ADR 0012: 共有するホスト API の型と宣言は `vy-host-api` に置く

- 状態: 採用（型の置き場所は有効。宣言の共有方法は
  [0013](./0013-host-api-traits.md) の trait に置き換え。生成の分離は
  [0014](./0014-api-codegen-cli.md) で実装）
- 日付: 2026-10-24

## 文脈

ADR 0009 は、プラグイン API の params / reply / イベント payload 型を
ハンドラの隣に置くと決めました。クライアントだけが API を持っていた間は、
`vy-app/src/api/namespaces/*.rs` に型とハンドラが並ぶ形で問題ありませんでした。

しかし Phase 6 では `backend.ts` が `vy-server` で動きます。`workspace`、`fs`、
`process`、`document`、`config.get` などの API は、クライアントとコアの両方に
同じワイヤー型と宣言が必要になります。`vy-server` は GPUI に依存できないため、
`vy-app` の型をそのまま使うことはできません。型を複製すると、TypeScript と
Rust の両方でずれが生まれます。

また、クライアント専用の `ui`、`views`、`panes`、`keymap`、`commands`、
`editor` のクライアント状態まで共有すると、サーバーが UI の語彙を持つことになり、
依存方向が崩れます。

## 決定ドライバ

1. 共有 API の型と宣言を1か所に保つこと。
2. `vy-server` が GPUI に依存しないこと。
3. クライアント専用 API をサーバーへ漏らさないこと。
4. 既存の TypeScript 生成を壊さず、段階的に移行できること。
5. ハンドラはホストごとの状態に合わせて別実装にできること。

## 決定

### 1. 共有する型と宣言は `vy-host-api` に置く

`vy-host-api` は次のものを持ちます。

- params / reply / イベント payload の型
- メソッド名、JSDoc、型参照を持つ `MethodDecl`
- 名前空間の `NamespaceDecl`
- 共有イベントの `EventDecl`

ハンドラは置きません。クライアントは `vy-app/src/api`、コアは
`vy-server/src/api` に実装します。

### 2. 名前空間は trait として宣言する

共有APIは `vy-host-api` の trait、クライアント専用APIは `vy-client-api` の trait
として宣言します。`vy-app` と `vy-server` は trait を実装し、`host_api!` が
facade と dispatch を生成します。詳細は
[0013](./0013-host-api-traits.md) にあります。

これにより、ワイヤー名・params / reply 型・JSDoc の正は API クレートの1か所に
残ります。

### 3. クライアント専用 API は `vy-client-api` に置く

`ui`、`views`、`panes`、`keymap`、`commands`、`editor` のクライアント状態、
`config.set`、`workspace.state`、`backend.connect` はクライアント専用です。
これらの型と宣言は `vy-client-api` に置き、ハンドラだけを `vy-app` に残します。

`vy-client-api` は GPUI に依存しません。`ThemeColor`、`WidgetNode`、`PickerItem`、
`Mode`、`StatuslineAlignment` のような API 境界を越える値型もここに置き、
`vy-app` の対応するモジュールから再エクスポートします。

### 4. 型生成の入口

現在は `vy-api-codegen` が、共有宣言を参照するクライアントの `methods()` と、
クライアント専用名前空間の宣言をまとめて `packages/vy-api` と
`packages/vy-api-types` に書きます（[0014](./0014-api-codegen-cli.md)）。

バックエンド用の `backend.d.ts` を出す段階でも、同じ `codegen::export_to` に
`vy-host-api` の宣言とバックエンド専用宣言を渡します。実行時には誰も
`vy-api-codegen` に依存しません。

### 5. バックエンド実装

`vy-server` は `vy-host-api` の trait をコア用のホストに実装します。
ケイパビリティの検査はコア側で行います。

## 今回の範囲

API 宣言の切り出しを完了しました。

- `vy-host-api` に共有API（`fs`、`workspace`、`process`、`document`、
  `config.get` / `config.inspect`、`log`、`context`）を追加
- `vy-client-api` にクライアント専用API（`ui`、`ui.statusline`、`ui.gutter`、
  `ui.hover`、`ui.picker`、`views`、`keymap`、`commands`、`editor`、
  `notifications`、`config.set`）を追加
- API 境界を越える値型（`ThemeColor`、`WidgetNode`、`PickerItem`、`Mode`、
  `StatuslineAlignment`）を `vy-client-api` に移し、`vy-app` から再エクスポート
- `DocumentInfoReply` を `vy-host-api` に移し、`vy-js` から再エクスポート
- `vy-app` の各 namespace はハンドラだけを持つ
- 生成される TypeScript の型名とワイヤー名は変えない

`vy-api-codegen` は [0014](./0014-api-codegen-cli.md) で追加しました。
`backend.d.ts` は次の段階です。

## 帰結

### 良い面

- 共有 API の型と宣言が1か所になり、クライアントとコアでずれない。
- `vy-server` が `vy-app` に依存せずに同じ API を実装できる。
- クライアント専用 API がサーバーへ漏れない。
- 既存の生成物を変えずに移行できる。

### 悪い面

- クレートが1つ増え、API を追加するときに共有宣言とハンドラの2か所を見る。
- ハンドラの params 型が共有宣言と一致しているかは、コンパイル時だけでは
  完全には検証できない。

### 緩和策

- 共有宣言の名前はハンドラ関数名の大文字（`read_file` → `READ_FILE`）に揃える。
- 生成物の差分がないことを `cargo run -p vy-api-codegen -- client --check` で確認する。
- 将来、共有宣言からハンドラの型を検査するテストを足す。

## 見直しの条件

1. `vy-client-api` を切る必要が出たとき。
2. バックエンド用の型生成を `vy-app` から分離するとき。
3. 共有 API のハンドラ実装がクライアントとコアで大きく異なり、宣言の共有が
   足枷になるとき。

## 関連する未決定事項

- 共有宣言とハンドラの型の一致をどこまで静的に検査するか。
- `vy-api-macros` を declarative なハンドラ表に置き換えるか。
