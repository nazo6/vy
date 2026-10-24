# ADR 0015: API の生成物は `packages/vy-api` にまとめる

- 状態: 採用
- 日付: 2026-10-24

## 文脈

ADR 0007 は、プラグイン境界の型を `packages/vy-api-types`、名前空間の宣言と
糖衣を `packages/vy-api` に分けました。当時は型を `ts-rs` のテストが書き、
名前空間を別の codegen が書いていたため、生成元が2つありました。

ADR 0009 と [0014](./0014-api-codegen-cli.md) で生成元は
`vy-api-codegen` に一本化されました。それでも `packages/vy-api-types` は
`package.json` を持たない別ディレクトリのままで、次の問題がありました。

- `vy-api` パッケージの外側を相対 import している
- 公開時に `vy-api-types` をパッケージへ含められない
- 型と宣言の対応を追うときに2つのディレクトリを行き来する
- `TS_RS_EXPORT_DIR` が暗黙の出力先になり、CLI の `Output` と二重になる

## 決定ドライバ

1. 生成物と手書きの糖衣を1つのパッケージにまとめること。
2. 生成の出力先を、環境変数ではなくコードと CLI で決めること。
3. TypeScript の import と TypeDoc の入口を単純にすること。
4. 将来のバックエンド用生成を追加する余地を残すこと。

## 決定

### 1. `packages/vy-api/types/` に生成型を置く

生成された per-type の `.ts`、`HostMethods.ts`、`HostEvents.ts`、`CommandId.ts`、
`SettingsKey.ts`、`types/index.ts` は `packages/vy-api/types/` に置きます。

手書きの `overrides.d.ts`、生成された `index.d.ts` と `docs.d.ts` は
`packages/vy-api/` の直下に置きます。`index.d.ts` は `./types` と
`./overrides` を re-export し、`overrides.d.ts` は `CommandId` を
`./types` から import します。

`packages/vy-api-types` は削除します。`tsconfig` と TypeDoc の include も
`packages/vy-api/**/*.ts` に揃えます。

### 2. `TS_RS_EXPORT_DIR` を廃止する

`TypeCollector` は `ts_rs::Config::default()` を使います。出力先は
`vy-api-schema::codegen::Output` が持ち、リポジトリの既定値は
`Output::packages()` が返します。`--check` は一時ディレクトリを `Output` に
渡します。

`TS_RS_EXPORT_DIR` は、`ts-rs` の自動テストに出力先を教えるための環境変数
でした。`vy-core` と `vy-proto` に残っていた `#[ts(export)]` も外し、codegen が
`Output.types_dir` に直接書くため、環境変数は不要です。これで `cargo test` が
生成物を書き換える経路もなくなります。

### 3. パッケージの公開面を1つにする

`packages/vy-api/package.json` の `files` に `index.d.ts`、`docs.d.ts`、
`overrides.d.ts`、`types`、`README.md` を入れます。型と宣言を同じパッケージに
含められるようになります。

## 帰結

### 良い面

- 型と名前空間の宣言が1つのパッケージにまとまる。
- 相対 import が `../vy-api-types` から `./types` になり、公開面が単純になる。
- 出力先が `Output` に一本化され、環境変数に依存しない。
- `--check` と通常生成が同じパス計算を使う。

### 悪い面

- 生成ファイルのパスが変わるため、既存のリンクやスクリプトを更新する必要がある。
- `packages/vy-api` 直下の生成物と `types/` の役割を把握する必要がある。
- バックエンド用の型を同じ `types/` に置くか分けるかは未決定。

### 緩和策

- `README.md` と `docs/development.md` に生成コマンドとパスを書く。
- `--check` を CI に入れ、パス変更後の生成物を検査する。
- バックエンド用は、必要になった時点で `Output` を分けて追加する。

## 見直しの条件

1. バックエンド用の型をクライアントの `types/` と分ける必要が出たとき。
2. `vy-api` を実際に npm へ公開するとき。
3. 生成物の数が増え、`types/` のフラット配置が扱いにくくなったとき。

## 関連する未決定事項

- `backend.d.ts` とバックエンド型の出力先。
- `types/index.ts` の barrel を公開 API として維持するか。
