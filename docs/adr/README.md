---
title: "アーキテクチャ決定記録 (ADR)"
type: "guide"
updated_at: 2026-10-25
---

# アーキテクチャ決定記録 (ADR)

Vy の設計上の決定を記録します。1つの決定につき1ファイルとし、決定を変えるときは既存のファイルを書き換えず、新しい ADR を追加します。古い ADR には状態だけを足し、「置き換え済み」と分かるようにします。

## 命名規則

ファイル名は `NNNN-<short-title>.md` とします。番号は4桁で、追加した順に増やします。

## frontmatter 規約

各 ADR の先頭には以下の frontmatter を付与します。

```yaml
---
id: "0023"
title: "型生成を ts-rs から specta に移行し単一ファイルへ集約する"
type: "adr"
status: "accepted" # "proposed" | "accepted" | "superseded" | "rejected"
date: 2026-10-25
supersedes: ["0015", "0016"] # 任意: 置き換えた過去のADR番号
superseded_by: null         # 任意: 置き換えられた新しいADR番号
tags: ["api", "codegen"]
---
```

## トピック別・現在有効な決定 (Current Decisions)

「今どの決定が有効なのか」をトピックごとにまとめた一覧です。

| トピック | 現在有効な決定 (ADR) | 置き換えられた過去の ADR |
| :--- | :--- | :--- |
| **プロセス・通信モデル** | [0002](./0002-process-model.md) (別プロセス)<br>[0018](./0018-typed-rpc.md) (型付き RPC)<br>[0010](./0010-remote-connection.md) (SSH/再同期) | - |
| **状態・アンドゥ管理** | [0003](./0003-editor-state.md) (編集状態の分離)<br>[0004](./0004-undo-in-core.md) (コア所有アンドゥ) | 0003 のアンドゥ部分は 0004 で置き換え |
| **UI・描画** | [0001](./0001-ui-framework.md) (GPUI)<br>[0005](./0005-native-plugin-ui.md) (ネイティブ木)<br>[0006](./0006-user-controlled-ui-and-slots.md) (スロット) | - |
| **プラグイン API・基盤** | [0019](./0019-global-vy-api.md) (グローバル vy 統一)<br>[0020](./0020-plugin-source-and-bundles.md) (ソース/バンドル)<br>[0021](./0021-bundle-validation.md) (バンドル検証)<br>[0011](./0011-backend-plugins-on-core.md) (バックエンドプラグイン) | 0016 の API 入口は 0019 で置き換え |
| **型生成・ホスト API 実装** | [0009](./0009-api-types-live-with-handlers.md) (型の配置)<br>[0012](./0012-shared-host-api.md) (vy-host-api)<br>[0013](./0013-host-api-traits.md) (名前空間 trait)<br>[0023](./0023-specta-api-types.md) (Specta 型生成) | 0007 / 0008 は 0013 で置き換え<br>0014 / 0015 / 0016 は 0023 で集約・置き換え |
| **コマンド・CLI** | [0017](./0017-cli-parsing-with-usage-rs.md) (usage-rs)<br>[0022](./0022-command-surfaces.md) (コマンド面分離) | - |

---

## 全 ADR 索引 (番号順)

| 番号 | 決定 | 状態 |
| :--- | :--- | :--- |
| [0001](./0001-ui-framework.md) | UI フレームワークに GPUI と gpui-kit を採用する | 採用 |
| [0002](./0002-process-model.md) | クライアントとコアを常に別プロセスにする | 採用 |
| [0003](./0003-editor-state.md) | 編集状態をコアとクライアントに分ける | 採用（アンドゥは 0004 で置き換え） |
| [0004](./0004-undo-in-core.md) | アンドゥの履歴はコアが持つ | 採用 |
| [0005](./0005-native-plugin-ui.md) | プラグイン UI はネイティブのウィジェット木で作る | 採用 |
| [0006](./0006-user-controlled-ui-and-slots.md) | ユーザー制御 UI とスロットによるステータスライン | 採用 |
| [0007](./0007-rust-typed-plugin-api.md) | プラグイン境界の型は Rust を正とする | 採用（メソッド表の一部は 0008 で置き換え） |
| [0008](./0008-declare-plugin-api-with-macros.md) | プラグイン API を宣言マクロで一本化する | 置き換え済み（0013） |
| [0009](./0009-api-types-live-with-handlers.md) | API の型はハンドラの隣に置き、生成を集約する | 採用 |
| [0010](./0010-remote-connection.md) | SSH で接続し、再起動と再同期でセッションを戻す | 採用 |
| [0011](./0011-backend-plugins-on-core.md) | バックエンドプラグインはコアで動かす | 採用（実装は Phase 6。型の分割は 0019 で延期） |
| [0012](./0012-shared-host-api.md) | 共有するホスト API の型と宣言は `vy-host-api` に置く | 採用（型の置き場所。宣言方法は 0013） |
| [0013](./0013-host-api-traits.md) | 名前空間 trait と `HostApi` facade で実装する | 採用（すべての名前空間） |
| [0014](./0014-api-codegen-cli.md) | API バインディングの生成を CLI に分離する | 採用（出力レイアウトは 0015 で置き換え） |
| [0015](./0015-api-output-layout.md) | API の生成物は `packages/vy-api` にまとめる | 採用（一部は 0016 で置き換え） |
| [0016](./0016-command-types-and-ts-shim.md) | コマンド型を分離し、JS API の型を TS shim から生成する | 採用（API の入口は 0019 で変更） |
| [0017](./0017-cli-parsing-with-usage-rs.md) | CLI 引数の解析に usage-rs を使う | 採用 |
| [0018](./0018-typed-rpc.md) | クライアント・コア間の RPC は型付き marker で送る | 採用 |
| [0019](./0019-global-vy-api.md) | プラグイン API はグローバル `vy` に統一する | 採用（実装済み） |
| [0020](./0020-plugin-source-and-bundles.md) | プラグインはソース形態とバンドル形態の2つを許す | 採用（実装済み。検証の詳細は 0021） |
| [0021](./0021-bundle-validation.md) | バンドルとエントリをロード時に検証する | 採用 |
| [0022](./0022-command-surfaces.md) | コマンドの入口を役割で分け、設定を状態として分離する | 採用 |
| [0023](./0023-specta-api-types.md) | 型生成を ts-rs から specta に移行し単一ファイルへ集約する | 採用 |
