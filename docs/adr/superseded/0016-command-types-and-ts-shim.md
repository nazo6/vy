---
id: "0016"
title: "コマンド型を分離し、JS API の型を TS shim から生成する"
type: "adr"
status: "superseded"
category: "architecture"
date: 2026-10-24
supersedes: ["0014", "0015"]
superseded_by: "0025"
---

# ADR 0016: コマンド型を分離し、JS API の型を TS shim から生成する

> この決定は [ADR 0025](../0025-plugin-api-declaration-and-codegen.md) に引き継ぎました。
> 実装と検証の記録は [worklog](../../worklogs/2026-10-24-plugin-api-pipeline.md) にあります。

- 状態: 採用（API の入口は [0019](../0019-global-vy-api.md) でグローバル `vy` に
  置き換え）
- 日付: 2026-10-24

## 文脈

`commands.registerCommand` は独自コマンドを登録する API ですが、その型は
`CommandId | (string & {})` でした。`CommandId` は `vy.*` の組み込みコマンドだけを
並べた union で、`registerCommand` では予約済みの ID になります。

また、`commands.execute` は `CommandMap` を使った overload を持っていましたが、
一般的な `{ id: string; args: unknown }` の overload にフォールバックするため、
既知のコマンドに誤った引数を渡しても型エラーになりませんでした。

`commands.registerCommand` の callback と `commands.execute` の TS 向け overload は
`overrides.d.ts` に手書きし、`#[vy_api_trait(sugar = ...)]` で生成 interface に
継承させていました。

## 決定

### 1. 組み込みコマンドと独自コマンドの型を分ける

- `BuiltinCommandMap`: `vy.*` の組み込みコマンドと引数。Rust から生成する。
- `BuiltinCommandId`: `keyof BuiltinCommandMap`。
- `CommandMap`: プラグインが宣言マージで追加する独自コマンド。

`commands.registerCommand` は `CommandMap` だけを使い、`CommandId` は使いません。

```ts
declare module "vy-api" {
  interface CommandMap {
    "user.add": { a: number; b: number };
  }
}
```

### 2. 既知のコマンドは conditional type で引数を検査する

単純な overload と `string` フォールバックの組み合わせでは、既知の ID でも
フォールバックが選ばれて型検査をすり抜けます。そのため、`const K` と
conditional type を使います。

```ts
type ExecuteParams<K extends string> = K extends keyof BuiltinCommandMap
  ? { id: K; args?: BuiltinCommandMap[K] }
  : K extends keyof CommandMap
    ? { id: K; args: CommandMap[K] }
    : { id: K; args?: unknown };

function execute<const K extends string>(
  params: ExecuteParams<K>,
): Promise<unknown>;
```

`registerCommand` も同じ形で handler の引数を検査します。組み込み ID を
`registerCommand` に渡すと型エラーになります。`keymap.bind` の `command` も
`BuiltinCommandId | keyof CommandMap` を使います。

### 3. JS API の型は TS shim から生成する

Rust の codegen は `types/` と `api-base.d.ts` を生成します。`api-base.d.ts` は
namespace interface と wire の型だけで、`commands.registerCommand` や
`commands.execute` のような JS 専用メソッドは `manual` として除外します。

JS 専用メソッドは `crates/vy-js/src/shim.ts` に実装し、その実装から
`tsc --declaration` で `packages/vy-api/shim.d.ts` を生成します。

```text
Rust codegen
  ├─ packages/vy-api/types/*.ts
  └─ packages/vy-api/api-base.d.ts

shim.ts
  ├─ tsc -> target/api-shim/crates/vy-js/src/shim.d.ts
  │         -> packages/vy-api/shim.d.ts
  └─ tsc -> target/api-shim/crates/vy-js/src/shim.js
            -> crates/vy-js/src/generated/shim.js

packages/vy-api/index.d.ts
  ├─ export * from "./types"
  ├─ export * from "./api-base"
  └─ export * from "./shim"
```

`overrides.d.ts`、`sugar` 属性、`typed_args` は削除します。

### 4. 生成コマンド

```sh
pnpm generate:api
```

内部では `vy-api-codegen` と `tsc` を順に実行します。`vy-api-codegen` の
`--check` は Rust が生成する `types/`、`api-base.d.ts`、`index.d.ts` を検査し、
`tsc` の出力は `git diff` で検査します。

## 帰結

### 良い面

- `registerCommand` から組み込み専用の `CommandId` が消える。
- `CommandMap` を拡張すると、登録と実行の両方で引数が型付けされる。
- 既知のコマンドに誤った引数を渡すと型エラーになる。
- shim の実装と型が同じ TS ファイルにあり、`overrides.d.ts` が不要になる。
- callback の型を Rust の wire 型から無理に推論しないで済む。

### 悪い面

- `tsc` と Node が生成工程に入る。
- `shim.ts` は `api-base.d.ts` に依存するため、生成順序が必要になる。
- 組み込みコマンドの引数はまだ `unknown` で、ID の補完だけが型付きになる。
- `commands.execute` の戻り値はまだ型付けしていない。ホストが結果を返す経路が
  必要になる。

### 緩和策

- `pnpm generate:api` を 1 つの入口にし、生成物の先頭にコマンドを書く。
- `type-tests/commands.ts` で `@ts-expect-error` を使い、型検査の回帰を防ぐ。
- `tsc` の出力は `packages/vy-api/shim.d.ts` と
  `crates/vy-js/src/generated/shim.js` に限定する。

## 見直しの条件

1. 組み込みコマンドの引数をすべて型付けするとき。
2. `commands.execute` の戻り値をホストから返すとき。
3. callback 系 API が増え、`RegisterHandler` 以外の sugar 型が必要になったとき。
4. shim の生成に別のバンドラやビルドシステムを使うとき。

## 関連する未決定事項

- 組み込みコマンドの引数型をどこまで精密にするか。
- コマンド実行結果を `CommandMap` に含めるか。
- `CommandMap` を `{ args, result }` の形に変更するか。
