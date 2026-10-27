---
title: "プラグイン API の型・宣言・生成パイプライン整備ログ"
type: "worklog"
date: 2026-10-24
phase: "Phase 2-5"
tags: ["api", "codegen", "plugin", "migration"]
---

# [Worklog] プラグイン API の型・宣言・生成パイプライン整備ログ

- **日付**: 2026-10-24（移行の完了は 2026-10-25）
- **関連フェーズ/タスク**: Phase 2（プラグイン基盤）、Phase 5（プラグイン API の仕上げ）
- **関連 ADR**: [0025](../adr/0025-plugin-api-declaration-and-codegen.md)（集約後の決定）。移行時の個別 ADR は [`../adr/superseded/`](../adr/superseded/README.md) にあります。
- **目的**: プラグイン境界の型・宣言・TypeScript 生成が 3 箇所に分裂していた状態を、単一情報源と 1 つの生成入口に移す。ここには移行の手順、検証結果、つまずいた点を残します。

## 作業内容

### 1. スキーマ crate とメソッド表（旧 0007）

- `crates/vy-api-schema` を新設し、境界の値型（`Mode`、`ThemeColor`、`StatuslineAlignment`、`Gap`、`Font`、`ButtonVariant`、`WidgetNode`）と params / reply 型、`HOST_METHODS` を集めた。
- `plugin_host.rs` の文字列 match を `HostCall` の網羅 match に置き換え、腕ごとに `parse_params::<T>` で復元する形にした。`null` は `{}` として扱い、欠落と型違いは `InvalidParams` にした。
- shim をメソッド表から組み立てる汎用フォワーダに変え、620 行から 451 行に減らした。糖衣（`Document`、`editor.edit`、statusline / gutter ハンドル、picker、views、widget ビルダー）だけを手書きで残した。
- 色・配置・gap・フォント・variant・ログレベルを列挙にし、描画側の文字列 match を網羅 match に置き換えた。`color_for_token` は削除した。

### 2. 宣言マクロと名前空間 trait（旧 0008 / 0012 / 0013）

- 最初は属性マクロで `mod` 内の関数を列挙する方式にした。PoC を `editor.mode` / `editor.setMode` で行い、Phase 2 で全メソッドを移して旧 dispatch を削除した。
- その後、param 型をハンドラの隣へ移す段階で、宣言を名前空間 trait に変えた。`#[vy_api_trait]` が `METHODS` / `NAMESPACE` / `EVENTS` / `dispatch` を生成し、`host_api!` が facade をまとめる。
- `vy-host-api`（共有）と `vy-client-api`（クライアント専用）に分割した。`vy-server` は `vy-host-api` の trait だけを `CoreHost` に実装する。
- イベントは関連定数で宣言し、`HostEvent` の marker で `emit_event::<T>` を呼ぶ形にした。topic 文字列の組み立ては消えた。

### 3. 生成の入口と出力（旧 0009 / 0014 / 0015 / 0016）

- `codegen::export` を `vy-app` のテストから呼ぶ形をやめ、`vy-api-codegen` CLI に移した。依存は API クレートだけで、GPUI と `vy-app` を含めない。
- `--check` は一時ディレクトリに生成して比較する。テストはリポジトリへ書き込まなくなった。
- 出力を `packages/vy-api` にまとめた。`TS_RS_EXPORT_DIR` と `packages/vy-api-types` は廃止した。
- コマンド型を `BuiltinCommandMap` と `CommandMap` に分け、`overrides.d.ts` を削除した。JS 専用メソッドの型は `shim.ts` から `tsc --declaration` で出し、`pnpm generate:api` を唯一の入口にした。

### 4. Specta への移行（旧 0023）

- `ts-rs` を `specta` + `specta-typescript` に置き換えた。
- 型ごとの約 95 ファイルと `types/index.ts` の barrel をやめ、`types/bindings.ts` の 1 ファイルに集約した。`types/index.ts` は再エクスポートだけの薄いファイルとして残し、`./types` 参照を壊さないようにした。
- フィールド単位の doc コメントが JSDoc として出るようになった。

## 発生した課題と対処

- **`Option<T>` / `Vec<T>` の型取りこぼし**: `ts-rs` の `visit_dependencies` は `T` 自体を訪れないため、`Option<DocumentInfoReply>` から `DocumentInfoReply.ts` が生成されなかった。`visit_generics` も辿る collector を書いて回避したが、Specta への移行で collector ごと不要になった。
- **`Mode` の serde alias**: `ts-rs` が alias を解釈できず警告を出したため、`Mode::parse` を使う独自 `Deserialize` にした。Specta 移行後もこの形を維持している。
- **ケイパビリティと `file:` URI**: 宣言は `${workspace}/**` のようなパス、API は `file:` URI を渡すため、そのままでは glob が一致しない。クライアントと `CoreHost` の両方で URI をパスへ変換してから照合するようにした。
- **テストが生成物を書く**: テストの実行順で生成結果が変わり、`vy-app` のビルドが生成に必要だった。CLI 化と `--check` で解消した。
- **`--check` の対象外**: `--check` は `types/` と `api-base.d.ts` だけを比較し、`shim.d.ts` と `generated/shim.js` は `git diff` で検査する。CI が無いため、手元の検証で回している。
- **`shim.ts` と `bindings.ts` の型重複**: 生成側の `CommandArgInfo` は `choices` / `required` / `rest` が非オプショナルなので、単純な置換では合わない。`CommandsRegisterParams` を受け、`Partial` で受ける必要がある（残課題）。

## 検証結果・測定値

- 移行完了時（旧 0007 の Phase E）: ワークスペース全体で 244 テスト、clippy / fmt 緑、生成物の再現性 OK、`examples/init.ts` の `tsc` 通過。
- TypeScript の型検査は、リポジトリに TS のセットアップが無い時期に `/tmp` のフィクスチャで行った（`npx -p typescript@5 tsc`）。現在は `pnpm check:types` と `pnpm generate:api` がある。
- 生成物の差分は `cargo run -p vy-api-codegen -- client --check` と `git diff --exit-code packages` で確認した。
- Phase A〜E のコミット: 5f2fb45（A）、3297023（B）、acdfdfe（C）、f4626ce（D）、3b26eb0（整理）、5b72472（E）。Specta 移行は 41f36de。

## フォローアップ・残課題

- `backend.d.ts` の出力と、コア限定の型をクライアントと共有するか（Phase 6、[ADR 0026](../adr/0026-phase-6-remote-plugins.md)）。
- 実行時の引数スキーマ（型は Oxc で除去されるため、誤った引数は実行時まで分からない）。
- `shim.ts` の手書き型と `bindings.ts` の重複の解消。
- `CommandMap` を複数のプラグインが拡張したときの規則。
