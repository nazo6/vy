---
title: "置き換えられた ADR"
type: "guide"
updated_at: 2026-10-27
---

# 置き換えられた ADR

現行の決定に引き継がれた ADR です。番号と本文は履歴として残しますが、**決定の正ではありません**。現在の決定は [`../README.md`](../README.md) の索引を参照してください。

本文からは実装の記録（移行手順、検証した範囲、つまずき）だけを削除し、決定・文脈・帰結は残しています。削除した内容は [`../../worklogs/2026-10-24-plugin-api-pipeline.md`](../../worklogs/2026-10-24-plugin-api-pipeline.md) にあります。

| 番号                                             | 決定                                                   | 引き継ぎ先                                            |
| :----------------------------------------------- | :----------------------------------------------------- | :---------------------------------------------------- |
| [0007](./0007-rust-typed-plugin-api.md)          | プラグイン境界の型は Rust を正とする                   | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0008](./0008-declare-plugin-api-with-macros.md) | プラグイン API を宣言マクロで一本化する                | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0009](./0009-api-types-live-with-handlers.md)   | API の型はハンドラの隣に置き、生成を集約する           | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0012](./0012-shared-host-api.md)                | 共有するホスト API の型と宣言は `vy-host-api` に置く   | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0013](./0013-host-api-traits.md)                | 名前空間 trait と `HostApi` facade で実装する          | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0014](./0014-api-codegen-cli.md)                | API バインディングの生成を CLI に分離する              | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0015](./0015-api-output-layout.md)              | API の生成物は `packages/vy-api` にまとめる            | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0016](./0016-command-types-and-ts-shim.md)      | コマンド型を分離し、JS API の型を TS shim から生成する | [0025](../0025-plugin-api-declaration-and-codegen.md) |
| [0023](./0023-specta-api-types.md)               | 型生成を ts-rs から Specta に移行する                  | [0025](../0025-plugin-api-declaration-and-codegen.md) |

## 運用

- 新しく決定を置き換えたときは、古い ADR をこのディレクトリへ `git mv` で移し、`status: "superseded"` と `superseded_by` を設定します。番号は変えません。
- 移動した ADR は現行の索引（[`../README.md`](../README.md)）に載せません。この索引にだけ追加します。
- 部分的な置き換え（例: 0003 のアンドゥ部分を 0004 が置き換え）は、ファイルを移さず本文と索引の注記で表します。
