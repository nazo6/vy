---
title: "開発方針"
type: "guide"
updated_at: 2026-10-25
---

# 開発方針

このファイルは、リポジトリで作業するときの共通の決まりをまとめたものです。
画面の設計は [`specs/architecture.md`](./specs/architecture.md) にあります。

## 1. 言語

| 対象                                                 | 言語   |
| :--------------------------------------------------- | :----- |
| `README.md`、`docs/` の文書                          | 日本語 |
| コード中のコメントと doc コメント                    | 英語   |
| ログ、エラーメッセージ、利用者に見せる文字列         | 英語   |
| コミットメッセージと PR の説明                       | 英語   |
| テストデータ（UTF-16 や IME の検証に使う文字列など） | 何でも |

コードに日本語の文言を埋め込まないのは、将来の多言語化で文言を差し替えられるように
するためです。利用者に見せる文字列は、コード内の英語の文言ではなく、表示の層で
メッセージキーから引く形に広げる予定です。

## 2. エラー

エラーは、機械が扱う値と、人が読む文言を分けます。

- 機械が扱う値は、安定したコードにします。プロトコルは
  [`ErrorCode`](../crates/vy-proto/src/error.rs) を使います。
- 文言は英語で書き、ログと API の応答に載せます。
- 利用者向けの表示は、コードから文言を引いて多言語化します。コードそのものは
  翻訳しません。
- Vim の `E123` のような番号だけを識別子にしません。検索しにくく、コードと番号の
  2 つの体系を持つことになるためです。番号が必要な場合も、`vy.pane.last`
  のような 安定した文字列のコードを先に決めます。

現時点では、プロトコルのエラーは `ErrorCode` と英語の文言、クライアントの内部の
失敗は英語の文言だけを持ちます。多言語化を始めるときに、文言のキーを足します。

## 3. 状態の所在

状態をどちらが持つかは [ADR 0003](./adr/0003-editor-state.md) と
[ADR 0004](./adr/0004-undo-in-core.md) で決めています。

- 本文、version、アンドゥの履歴、診断、レイアウトはコア。
- カーソル、選択、モード、スクロール、`ui` ビューの木はクライアント。
- 位置は境界で UTF-16 に変換します。コアの内部は char index です
  （[`specs/architecture.md`](./specs/architecture.md) の「位置の表現」）。

クライアントとコアは、ローカル接続でも別プロセスです
（[ADR 0002](./adr/0002-process-model.md)）。近道を作らず、常に RPC を通します。

## 4. ビルドと依存

- GPUI
  はバージョンを完全に固定します（[ADR 0001](./adr/0001-ui-framework.md)）。
  更新は独立した変更にします。
- GPUI と wgpu は最適化なしだと 1 フレームに数十 ms かかります。`Cargo.toml` の
  `[profile.dev.package."*"]` で依存だけ最適化しています。依存を変えた直後は
  ビルドが長くなります。
- `vy-server` のログは必ず標準エラーに出します。標準出力は RPC
  のフレーム専用です。

## 5. 検証

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
pnpm check:types
pnpm format:check
```

Rust 以外のファイルの整形は §8 に従います。

- コアの変更は、ヘッドレスのテストで確かめます。
- プロトコルの変更は、`vy-proto` の往復テストと `vy-server`
  の結合テストを足します。
- クライアントの GPUI に依存しない部分は `cargo test -p vy-app --lib`
  で確かめます。キーマップ、設定、コマンド、`init.ts`
  のホストは画面なしで動きます。
- `init.ts` の相対 import は `cargo test -p vy-js` で、拡張子の補完、
  `index.*`、循環 import、ルート脱出、裸の specifier を確認します。
- プラグインのランタイム分離と所有者は `cargo test -p vy-app --lib` の
  `plugin_runtimes_keep_their_own_source_and_owner` で確認します。
- バンドルの import、サイズ、ハッシュ、パス制限、`engines.vy` は
  `cargo test -p vy-app --lib plugin::` で確認します。
- プラグイン API の型は `pnpm check:types` で確かめます。
  `type-tests/commands.ts` が `Vy.CommandMap` の宣言マージ、
  `vy.commands.registerCommand`、`vy.commands.execute`、`vy.keymap.bind`
  の型検査を含みます。
- プロトコル境界とプラグイン境界の型を変えたら、`pnpm generate:api` を実行し、
  `packages/vy-api` と `crates/vy-js/src/generated/shim.js` をコミットします。
  CI では `pnpm generate:api` の後に `git diff --exit-code packages crates/vy-js/src/generated`
  で差分がないことを確かめます。出力先は `codegen::Output::packages()` が決め、
  型は `packages/vy-api/types`、wire の名前空間宣言は `packages/vy-api/api-base.d.ts`
  に置きます。`index.d.ts` は `types/`、`api-base.d.ts`、`shim.d.ts` を合成し、
  ambient な `vy` と `Vy.CommandMap` を宣言します（[ADR 0019](./adr/0019-global-vy-api.md)）。
  クライアントとコアで共有する API の params / reply / イベント payload 型と
  メソッド宣言は `vy-host-api` に置きます。クライアント専用の型と宣言は
  `vy-client-api` に置きます。名前空間は `#[vy_api_trait]` で trait として宣言し、
  `vy-app` / `vy-server` が実装します。共有する値型も `vy-client-api` に置き、
  `vy-app` の対応するモジュール（`keymap`、`color`、`widget` など）から
  再エクスポートします。`host_api!` が生成する `HostApi` / `ClientHostApi` の
  `dispatch` から名前空間を呼びます。イベントは trait 内の `#[vy_api_event]`
  関連定数で宣言します（[ADR 0025](./adr/0025-plugin-api-declaration-and-codegen.md)）。
  `vy-api-schema` はマクロのランタイム型と codegen だけを持ちます。型は Specta が
  `packages/vy-api/types/bindings.ts` に、`HostMethods.ts`、`HostEvents.ts`、
  `api-base.d.ts`、`index.d.ts` は `vy-api-codegen` が生成します。
  `BuiltinCommandMap.ts` と `SettingsKey.ts` も同じ CLI が生成します。JS 向けの型は
  `crates/vy-js/src/shim.ts` から `tsc` で `packages/vy-api/shim.d.ts` に生成します。
  `shim.js` は評価時に `globalThis.vy` を代入するモジュールで、ユーザー向けの
  仮想モジュールではありません。
  生成はテストではなく `pnpm generate:api`（`vy-api-codegen` と `tsc`）で行います。
- 入力の重さを見るときは `VY_FRAME_LOG=1` で起動します。フレームの間隔と、要素の
  組み立て時間が標準エラーに出ます。
- `init.ts` の不具合は `~/.cache/vy/logs/init.log`（`VY_CACHE_HOME`
  で変更可）と状態行で確認します。

## 6. JS API ドキュメント

`packages/vy-api` の宣言から、TypeDoc で検索付きの HTML
リファレンスを生成できます。

```sh
pnpm install --frozen-lockfile
pnpm generate:api
pnpm docs:api
```

出力は `target/docs/vy-api/index.html` です。`pnpm docs:api:serve`
を使うと生成後に <http://localhost:8000> で表示できます。HTML は `target/`
配下に生成し、コミットしません。

- 入口は `packages/vy-api/docs.d.ts` です。API の型を再エクスポートします。
  `vy` は ambient 宣言なので、説明は `api-base.d.ts` と `shim.d.ts` の型から
  引きます。公開パッケージの export 面は変えません。
- メソッドの説明は、`crates/vy-host-api` / `crates/vy-client-api` の trait の doc
  コメントが正です。`pnpm generate:api` で `api-base.d.ts` に JSDoc として出ます。
- JS 向けの糖衣（`vy.commands.registerCommand`、`vy.commands.execute`、イベント購読）の
  説明は `crates/vy-js/src/shim.ts` に書きます。`shim.d.ts` はそこから生成されます。
- 型の説明は Rust の doc コメントから Specta が JSDoc として出します（ADR 0023）。
  フィールドやバリアント単位の説明も反映されます。
- コメントは英語で書きます（§1）。生成 HTML の確認は `pnpm docs:api`
  が警告なしで終わることと、主要な API ページに説明と例が出ることです。

## 7. ドキュメントの構成と運用

ドキュメントは目的とライフサイクルによって置き場所を明確に分離します。詳細は [`docs/README.md`](./README.md) にあります。

- **生きた仕様書 (Living Specs)**: `docs/specs/*.md`
  現在のコードベースの規範です。コードや利用者が参照する詳細（表、上限値、プロトコル、API 一覧）を置きます。一覧は [`docs/specs/README.md`](./specs/README.md) にあります。
- **アーキテクチャ決定・提案記録 (ADR)**: `docs/adr/`
  アイデア起草から提案・決定・実装完了までを一元管理する記録です（`category`: `roadmap` / `architecture` / `feature` / `maintenance`）。決定内容は書き換えず、変えるときは新しい ADR を追加します。置き換えた ADR は `docs/adr/superseded/` へ移します。
- **作業ログ (Worklogs)**: `docs/worklogs/`
  着手した作業の計画と実施ログ（日付、実測値、つまずき、移行手順）を記録します。

### ADR を書くときのチェックリスト

- 提案・決定は 1 ファイルに 1 テーマか。複数の決定が連鎖しているなら、現在有効な内容を新しい ADR に集約できないか。
- `status`（`draft` / `proposed` / `accepted` / `implemented` / `rejected` / `superseded`）と `category`（`roadmap` / `architecture` / `feature` / `maintenance`）を付与しているか。
- 実装の進捗、テスト名、実測値、移行手順が混ざっていないか（worklogs へ）。
- 決定を置き換える場合、置き換えられた ADR を `superseded/` へ移し、索引から外したか。

### メタデータ (frontmatter)

すべての Markdown ドキュメントの先頭には YAML frontmatter を付与します。

```yaml
---
title: "ドキュメントタイトル"
type: "spec" # "spec" | "adr" | "worklog" | "guide"
updated_at: 2026-10-27
---
```

## 8. フォーマット

整形の担当をファイル種別ごとに固定し、エディタと手元のコマンドで同じ結果になる
ようにします。設定はリポジトリ直下の `.prettierrc.json`、`tombi.toml`、
`.editorconfig`、`.vscode/settings.json` にあります。

| 対象                                         | ツール    | 設定                        |
| :------------------------------------------- | :-------- | :-------------------------- |
| Rust                                         | `rustfmt` | 既定（`rustfmt.toml` なし） |
| TypeScript、JavaScript、JSON、YAML、Markdown | Prettier  | `.prettierrc.json`          |
| TOML（`Cargo.toml`）                         | tombi     | `tombi.toml`                |

- インデントは 2 スペース、改行は LF、文字列はダブルクォート、文末はセミコロン
  です。Rust は `rustfmt` の既定（4 スペース）のままです。
- Markdown は `proseWrap: "preserve"` です。書いた行の折り返しを保ち、表の桁揃え
  だけを Prettier が行います。段落を 1 行にまとめず、読みやすい位置で手で折り返
  します。
- tombi のキー自動ソートは無効にしています。`Cargo.toml` はコメントで依存をグル
  ープ分けしており、ソートするとグループを跨いで移動するためです。
- `pnpm format` は Prettier と tombi をまとめて実行します。`pnpm format:check`
  は書き換えずに確認します。
- シェルスクリプトと Python は対象外です。整形が必要になったら `shfmt` と
  `ruff format` を追加し、この節に追記します。
- `packages/vy-api` の生成物（`types/`、`api-base.d.ts`、`index.d.ts`、
  `shim.d.ts`）と `crates/vy-js/src/generated` は Prettier の対象外です
  （`.prettierignore`）。生成側の出力形式が正で、`pnpm generate:api` の再現性を
  崩さないためです。生成物は生成元（Rust の trait、`crates/vy-js/src/shim.ts`）を
  直します。
- 既存のファイルは `pnpm format` で整形済みです。非 Rust
  ファイルを編集したら、保存時の整形に任せるか `pnpm format`
  を実行し、`pnpm format:check` で確認します。

### エディタ

- VS Code は `.vscode/settings.json` で保存時の整形を有効にしています。Rust は
  rust-analyzer、TOML は Tombi、それ以外は Prettier が担当します。
- 推奨拡張は `.vscode/extensions.json` にあります（Prettier、rust-analyzer、
  Tombi）。
- VS Code 以外のエディタは `.editorconfig` の改行コードとインデントに従います。
