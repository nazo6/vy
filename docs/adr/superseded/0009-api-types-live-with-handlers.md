---
id: "0009"
title: "API の型はハンドラの隣に置き、生成は codegen に集める"
type: "adr"
status: "superseded"
category: "architecture"
date: 2026-10-24
supersedes: ["0007", "0008"]
superseded_by: "0025"
---

# ADR 0009: API の型はハンドラの隣に置き、生成は codegen に集める

> この決定は [ADR 0025](../0025-plugin-api-declaration-and-codegen.md) に引き継ぎました。
> 実装と検証の記録は [worklog](../../worklogs/2026-10-24-plugin-api-pipeline.md) にあります。

- 状態: 採用（[0007](./0007-rust-typed-plugin-api.md) の決定 1 と
  [0008](./0008-declare-plugin-api-with-macros.md) の型の置き場所を置き換え。
  生成の実行方法は [0014](./0014-api-codegen-cli.md) に移行）
- 日付: 2026-10-24

## 文脈

ADR 0007 はプラグイン境界の型を `crates/vy-api-schema` に集め、ADR 0008 は
メソッドの宣言をハンドラ関数へ移しました。この組み合わせには 2 つの問題が残りました。

1. **メソッドを 1 つ足すと 2 ファイルを編集する。** params / reply 型は
   `crates/vy-api-schema/src/calls.rs`（754 行）、ハンドラは
   `crates/vy-app/src/api/namespaces/*.rs` にあり、互いの対応が名前だけでした。
2. **TypeScript の生成が 2 系統ある。** 型ファイルは各 crate の `#[ts(export)]` が
   生成するテストが書き、`HostMethods.ts` と `index.d.ts` は `vy-app` の codegen
   テストが書いていました。

利用範囲を調べると、`vy-api-schema` に依存するのは `vy-app` と `vy-js` だけで、
`vy-js` が使う境界型は `DocumentInfoReply` の 1 つだけでした。params / reply 型は
その名前空間のハンドラ以外から参照されていません。

## 決定ドライバ

1. メソッドの追加が 1 ファイルで完結すること。
2. 型の定義と TypeScript の生成が 1 箇所にまとまること。
3. 移行の各段階で生成物が変わらないこと（差分ゼロで検証できること）。
4. マクロのランタイム型は proc-macro クレートに置けないという制約を満たすこと。
5. `vy-app` → `vy-js` の依存方向を壊さないこと。

## 検討した選択肢

| 選択肢                                               | 内容                                                           | 評価                                                                                              |
| :--------------------------------------------------- | :------------------------------------------------------------- | :------------------------------------------------------------------------------------------------ |
| A. 現状維持                                          | 型は schema、生成は ts-rs と codegen の 2 系統                 | 動くが、追加コストと生成箇所の分散が残る                                                          |
| B. 型だけハンドラの隣へ                              | 生成は 2 系統のまま                                            | 追加は 1 ファイルになるが、「生成箇所を 1 つ」は満たさない                                        |
| C. 型をハンドラの隣へ + 宣言が生成情報を持つ（採用） | `MethodDecl` に `TypeRef` を持たせ、codegen が型ファイルも書く | 追加 1 ファイル、生成 1 関数。型の定義場所と生成場所を分離できる                                  |
| D. トレイトで状態を抽象化しハンドラを schema へ      | `Host` trait を定義してハンドラを下位クレートへ移す            | trait が API をミラーし、メソッド追加が trait・impl・ハンドラの 3 箇所になる。`Call` の寿命も残る |
| E. `vy-api-schema` を削除                            | マクロが `crate::api::...` を直接吐く                          | クレートは減るが、マクロが `vy-app` 専用になる                                                    |

## 決定

### 1. params / reply / イベント payload は名前空間の宣言と同じモジュールに置く

`vy-host-api` / `vy-client-api` の名前空間 trait と同じモジュールに型を定義します。
JSDoc、serde 属性、`ts-rs` の派生がメソッド宣言の隣に並び、型と API の対応が
ファイル内で完結します（[0013](./0013-host-api-traits.md)）。

### 2. 共有する値型は API クレートの自然なモジュールに置く

`Mode` は `keymap`、`ThemeColor` は `color`、`WidgetNode` 一式は `widget`、
`StatuslineAlignment` は `statusline`、`PickerItem` は `palette` です。
API 境界を越える値型は `vy-client-api` に置き、`vy-app` の対応するモジュールから
再エクスポートします（[0012](./0012-shared-host-api.md)）。

### 3. `DocumentInfoReply` は `vy-host-api` に置く

`DocumentInfoReply` はクライアントとコアの両方が使うため、`vy-host-api` に置き、
`vy-js` から再エクスポートします（[0012](./0012-shared-host-api.md)）。

以前は `vy-js` が唯一の外部利用者で、`vy-app` は `vy-js` に依存できました。逆向きの依存は
作りません。`vy-js` は `ts-rs` を依存に持ちます。

### 4. 宣言が型の生成情報を持つ

```rust
pub struct TypeRef {
    pub name: fn() -> String,
    pub collect: fn(&mut TypeCollector),
}
```

`TypeCollector` は `ts-rs` の `visit_dependencies` と `visit_generics` の
**両方**を辿ります。`Option<T>` と `Vec<T>` の `visit_dependencies` は `T` 自身を
訪れないため、片方だけでは `Option<DocumentInfoReply>` の `DocumentInfoReply.ts`
を取りこぼします。`output_path()` がある型だけを `export_to_string` で書き出します。

### 5. 生成の入り口は `codegen::export` の 1 つ

`cargo test --workspace export_bindings` のうち、プラグイン API に関するテストは
`vy-app` の `export_bindings_plugin_api` だけです。型ファイル、barrel、
`HostMethods.ts`、`HostEvents.ts`、`index.d.ts`、`docs.d.ts` を同じ関数が書きます。
API 型からは `#[ts(export)]` を外し、ts-rs の自動テストには頼りません。

### 6. `vy-api-schema` の役割

残るのは `MethodDecl`、`EventDecl`、`NamespaceDecl`、`TypeRef`、
`TypeCollector`、`Json`、`ts_name` と `codegen` だけです。proc-macro クレートは
ランタイム型を export できないため、この小さなクレートは残します。将来名前を
`vy-api` に変えても構いません。

## 帰結

### 良い面

- メソッドの追加が 1 ファイルで完結する。
- TypeScript の生成が 1 関数に集まり、`used_types` の文字列トークン走査が不要になる。
- 型の定義場所（`vy-app` / `vy-js`）と生成場所（`vy-app` の codegen）を分けられる。
- 各移行段階で `git diff --exit-code packages` が通ることを確認できる。

### 悪い面・課題

- `vy-app` が `ts-rs` に依存する。
- collector の正しさが `ts-rs` の `visit_generics` の挙動に依存する。
- `#[ts(export)]` を外した型を collector が取りこぼすと、生成物からファイルが消える。

### 緩和策

- `Option` / `Vec` の内側を集める回帰テストを `registry.rs` に置く。
- 移行は名前空間ごとに分け、各コミットで生成物の差分ゼロを確認する。
- CI の `git diff --exit-code packages` で取りこぼしを検出する。

## 見直しの条件

- 別クレートから同じマクロを使う必要が出たとき（型のパスを crate オプションにする）。
- 宣言の数が増え、collector の走査や codegen の時間が問題になったとき。
- `ts-rs` の `visit_generics` / `export_to_string` の API が変わったとき。
- ヘッドレスなプラグインホストなど、ハンドラを `vy-app` の外で使う必要が出たとき。

## 関連する決定

- [0007](./0007-rust-typed-plugin-api.md): 境界の型は Rust を正とする。
- [0008](./0008-declare-plugin-api-with-macros.md): メソッドを宣言マクロで一本化する。
