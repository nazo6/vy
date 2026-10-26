---
id: "0023"
title: "型生成を ts-rs から specta に移行し単一ファイルへ集約する"
type: "adr"
status: "superseded"
date: 2026-10-25
supersedes: ["0009", "0015"]
superseded_by: "0025"
---

# ADR 0023: 型生成を ts-rs から specta に移行し単一ファイルへ集約する

> この決定は [ADR 0025](../0025-plugin-api-declaration-and-codegen.md) に引き継ぎました。
> 実装と検証の記録は [worklog](../../worklogs/2026-10-24-plugin-api-pipeline.md) にあります。

- 状態: 採用
- 日付: 2026-10-25

## 文脈

ADR 0007、0009、0014、0015 でプラグイン境界の型生成パイプラインを整備し、`vy-api-codegen` CLI から一括出力する仕組みを構築しました。しかし、`ts-rs` をベースとした実装には以下の技術的負債が残っていました。

1. **個別ファイル出力によるファイル数の肥大化**:
   `ts-rs` の設計上、型ごとに 1 つのファイル（`packages/vy-api/types/*.ts`、約95ファイル）が出力され、それらを `types/index.ts` の barrel で再エクスポートしていました。ファイル数が多く、管理や書き込みオーバーヘッドとなっていました。
2. **自前の型走査ハック（`TypeCollector`）の保守コスト**:
   `ts-rs` はテストでの個別出力を前提としており、CLI から一括出力するために `visit_dependencies` と `visit_generics` の両方を辿る自前の `TypeCollector`（ADR 0009 §4）を実装していました。これは `ts-rs` の内部 API に密結合しており、バージョンアップやネストした総称型で壊れやすい状態でした。
3. **フィールド単位の JSDoc コメントの欠落**:
   `ts-rs` では構造体のフィールド単位の doc コメントが TypeScript に反映されず、型レベルの説明にまとめて記述せざるを得ない制約がありました（`docs/development.md`）。

ADR 0007 の「見直しの条件」に挙げられていた「型生成に成熟した仕組み（specta など）へ移りたくなったとき」を満たし、また出力レイアウトの変更が許容されるため、`specta` への移行を実施します。

## 決定ドライバ

1. **保守性の向上**: 自前の型走査ハックを廃止し、推移的な型解決をライブラリの型レジストリに委ねること。
2. **出力の整理**: 90 以上の個別ファイルと barrel 生成を廃止し、単一ファイル（`bindings.ts`）に集約すること。
3. **DX の向上**: 構造体フィールド単位の Rust doc コメントを TypeScript の JSDoc として忠実に出力すること。
4. **互換性の維持**: `packages/vy-api/types/index.ts` から `bindings.ts` を再エクスポートし、既存の `./types` 参照パスを壊さないこと。

## 決定

### 1. `specta` および `specta-typescript` を採用する

`ts-rs` を廃止し、`specta`（v2）と `specta-typescript` に移行します。
境界に現れる型は `#[derive(specta::Type)]` を導出します。

`registry.rs` にあった `Json(Value)` などのカスタム型マッピングは、specta のアトリビュートで表現します：

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[specta(type = unknown)]
pub struct Json(pub serde_json::Value);
```

### 2. `TypeCollector` を廃止し `specta::Types` レジストリに一本化する

`MethodDecl` や `EventDecl` は `TypeRef` を通じて `specta::Types` にルート型を登録します。Specta が `Option<T>`、`Vec<T>`、ネストした構造体などの依存関係を自動的に再帰解決するため、自前の走査ロジックを全廃します。

### 3. 生成型を `packages/vy-api/types/bindings.ts` に集約する

- `specta_typescript::Typescript::default().export_to(&output.types_dir.join("bindings.ts"), &types)` を用いて全型を `types/bindings.ts` に書き出します。
- 既存の 90 以上の `types/*.ts` 個別ファイルは削除します。
- `types/index.ts` は `bindings.ts`、`HostMethods.ts`、`HostEvents.ts` を再エクスポートする薄いファイルにします。
- `api-base.d.ts` や `shim.ts` からの import は引き続き `./types` 経由で行えるため、既存の型チェックや利用側に影響を与えません。

## 帰結

### 良い面

- `TypeCollector` の自前トラバース（`visit_dependencies` / `visit_generics`）が消え、コードが大幅に単純化されます。
- 約95個の個別ファイルと巨大な barrel 生成が不要になり、生成処理の実行速度と管理性が向上します。
- 構造体のフィールドや enum バリアントの doc コメントが JSDoc として TypeScript 側に反映され、開発者体験が向上します。
- `serde` 属性（flatten, tag 等）のシミュレーション精度が高まり、型定義の不整合リスクが低減します。

### 悪い面・課題

- 各 API クレートの全型定義で `derive(TS)` を `derive(specta::Type)` に書き換える移行コストが発生します。
- 生成される TypeScript 型のフォーマットや定義順が変更されるため、初回移行時に大きな差分が発生します。

### 緩和策

- `types/index.ts` で再エクスポートを維持することで、TypeScript 側の import 文の大規模な書き換えを防止します。
- CI の `cargo run -p vy-api-codegen -- client --check` と `pnpm typecheck` により、型生成の整合性とリグレッションを自動検証します。

## 関連する決定

- [0007](./0007-rust-typed-plugin-api.md): プラグイン境界の型は Rust を正とする。
- [0009](./0009-api-types-live-with-handlers.md): API の型はハンドラの隣に置き、生成を集約する。
- [0014](./0014-api-codegen-cli.md): API バインディングの生成を CLI に分離する。
- [0015](./0015-api-output-layout.md): API の生成物は `packages/vy-api` にまとめる。
