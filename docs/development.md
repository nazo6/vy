# 開発方針

このファイルは、リポジトリで作業するときの共通の決まりをまとめたものです。
画面の設計は [`architecture.md`](./architecture.md)、実装の順序は
[`roadmap.md`](./roadmap.md) にあります。

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
  （[`architecture.md`](./architecture.md) の「位置の表現」）。

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
```

- コアの変更は、ヘッドレスのテストで確かめます。
- プロトコルの変更は、`vy-proto` の往復テストと `vy-server`
  の結合テストを足します。
- クライアントの GPUI に依存しない部分は `cargo test -p vy-app --lib`
  で確かめます。キーマップ、設定、コマンド、`init.ts`
  のホストは画面なしで動きます。
- プロトコル境界とプラグイン境界の型を変えたら、
  `cargo test --workspace export_bindings` を実行し、`packages/vy-api-types` を
  一緒にコミットします。CI では生成後に `git diff --exit-code packages`
  で差分がないことを確かめます。生成先は `.cargo/config.toml` の
  `TS_RS_EXPORT_DIR` です。プラグイン API の型とメソッド表は
  `crates/vy-api-schema` が持ち、`HostMethods.ts` と
  `packages/vy-api/index.d.ts` も同じテストで生成します。手で書くのは糖衣の
  `packages/vy-api/overrides.d.ts` だけです。`CommandId.ts` と `SettingsKey.ts`
  は `vy-app`
  のテストが生成します（[ADR 0007](./adr/0007-rust-typed-plugin-api.md)）。
- 入力の重さを見るときは `VY_FRAME_LOG=1` で起動します。フレームの間隔と、要素の
  組み立て時間が標準エラーに出ます。
- `init.ts` の不具合は `~/.cache/vy/logs/init.log`（`VY_CACHE_HOME`
  で変更可）と状態行で確認します。

## 6. JS API ドキュメント

`packages/vy-api` の宣言から、TypeDoc で検索付きの HTML
リファレンスを生成できます。

```sh
pnpm install --frozen-lockfile
cargo test --workspace export_bindings
pnpm docs:api
```

出力は `target/docs/vy-api/index.html` です。`pnpm docs:api:serve`
を使うと生成後に <http://localhost:8000> で表示できます。HTML は `target/`
配下に生成し、コミットしません。

- 入口は `packages/vy-api/docs.d.ts` です。`index.d.ts`
  を再エクスポートし、糖衣の interface も TypeDoc に見せます。公開パッケージの
  export 面は変えません。
- メソッドの説明は `crates/vy-api-schema/src/calls.rs` の `MethodSpec.doc`
  が正です。`cargo test --workspace export_bindings` で `index.d.ts` に JSDoc
  として出ます。
- 糖衣（`Document`、`editor.edit`、statusline、picker、views、widget builder）の
  説明は `packages/vy-api/overrides.d.ts` に書きます。
- 型の説明は Rust の doc コメントから `ts-rs` が JSDoc として出します。params
  型のフィールドは型レベルの説明に書きます（`ts-rs` はフィールド単位の JSDoc
  を出しません）。
- コメントは英語で書きます（§1）。生成 HTML の確認は `pnpm docs:api`
  が警告なしで終わることと、主要な API ページに説明と例が出ることです。
