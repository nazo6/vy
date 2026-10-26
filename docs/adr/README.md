---
title: "アーキテクチャ決定記録 (ADR)"
type: "guide"
updated_at: 2026-10-27
---

# アーキテクチャ決定記録 (ADR)

Vy の設計上の決定を記録します。1つの決定につき1ファイルとし、決定を変えるときは既存のファイルの決定内容を書き換えず、新しい ADR を追加します。置き換えられた ADR は [`superseded/`](./superseded/README.md) へ移し、この索引からは外します（番号は変えません）。

ADR には**決定とその理由だけ**を書きます。規範の詳細（表、上限値、プロトコル、API 一覧）は [`../specs/`](../specs/README.md)、実装の進捗・検証値・移行手順は [`../worklogs/`](../worklogs/README.md)、まだ決めていないことは [`../proposals/`](../proposals/README.md) に置きます。実装済みかどうかは置き場所を変えず、`implementation` で表します。

## 命名規則

- 現行の ADR は `docs/adr/NNNN-<short-title>.md` に置きます。番号は 4 桁で、追加した順に増やします。
- 置き換えられた ADR は `docs/adr/superseded/` へ移します。ファイル名と番号は変えません。
- 番号は再利用しません。索引に無い番号は、`superseded/` に移った ADR を指します。

## frontmatter 規約

各 ADR の先頭には以下の frontmatter を付与します。

```yaml
---
id: "0025"
title: "プラグイン API の宣言・型・生成を名前空間 trait と Specta に集約する"
type: "adr"
status: "accepted" # "proposed" | "accepted" | "rejected" | "superseded"
date: 2026-10-27
implementation: "done" # 任意: "done" | "partial" | "not-started"
supersedes: ["0007", "0008"] # 任意: 引き継いだ過去の ADR 番号
superseded_by: null # 任意: 引き継ぎ先の ADR 番号（決定全体を置き換えた場合のみ）
tags: ["api", "codegen"]
---
```

- `status: "superseded"` は「決定が新しい ADR に引き継がれた」ことを表します。内容が否定されたとは限りません。`superseded/` にある ADR はすべてこの状態です。
- `superseded_by` は決定全体を置き換えた場合だけ設定します。一部だけを置き換えた場合（例: 0003 のアンドゥ部分を 0004 が置き換え）は `null` のままにし、本文の「置き換え」とこの索引の補足で表します。
- `implementation` は実装状況です。ADR は実装前でも作り、この値で進捗を表します。

## トピック別・現在有効な決定 (Current Decisions)

「今どの決定が有効なのか」をトピックごとにまとめた一覧です。置き換え済みの ADR は含めません（[`superseded/`](./superseded/README.md)）。

| トピック                 | 現在有効な決定 (ADR)                                                                                                                                                                                                                         | 補足                                                                                 |
| :----------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------------------------------- |
| **プロセス・通信モデル** | [0002](./0002-process-model.md) (別プロセス)<br>[0018](./0018-typed-rpc.md) (型付き RPC)<br>[0010](./0010-remote-connection.md) (SSH/再同期)                                                                                                 | -                                                                                    |
| **状態・アンドゥ管理**   | [0003](./0003-editor-state.md) (編集状態の分離)<br>[0004](./0004-undo-in-core.md) (コア所有アンドゥ)                                                                                                                                         | 0003 のアンドゥ部分は 0004 で置き換え                                                |
| **UI・描画**             | [0001](./0001-ui-framework.md) (GPUI)<br>[0005](./0005-native-plugin-ui.md) (ネイティブ木)<br>[0006](./0006-user-controlled-ui-and-slots.md) (スロット)<br>[0024](./0024-keyboard-first-plugin-ui.md) (キーボードファーストなプラグイン UI)  | 0024 は実装未着手（[proposals/0007](../proposals/0007-keyboard-first-plugin-ui.md)） |
| **プラグイン API・基盤** | [0019](./0019-global-vy-api.md) (グローバル vy)<br>[0020](./0020-plugin-source-and-bundles.md) (ソース/バンドル)<br>[0021](./0021-bundle-validation.md) (バンドル検証)<br>[0011](./0011-backend-plugins-on-core.md) (バックエンドプラグイン) | 0011 は実装未着手（[proposals/0001](../proposals/0001-phase-6-remote-plugins.md)）   |
| **API の宣言・型・生成** | [0025](./0025-plugin-api-declaration-and-codegen.md) (名前空間 trait + Specta + codegen CLI)                                                                                                                                                 | 0007–0016・0023 を集約                                                               |
| **コマンド・CLI**        | [0017](./0017-cli-parsing-with-usage-rs.md) (usage-rs)<br>[0022](./0022-command-surfaces.md) (コマンド面分離)                                                                                                                                | -                                                                                    |

## 全 ADR 索引 (番号順)

| 番号                                                 | 決定                                                                 | 状態                               |
| :--------------------------------------------------- | :------------------------------------------------------------------- | :--------------------------------- |
| [0001](./0001-ui-framework.md)                       | UI フレームワークに GPUI と gpui-kit を採用する                      | 採用                               |
| [0002](./0002-process-model.md)                      | クライアントとコアを常に別プロセスにする                             | 採用                               |
| [0003](./0003-editor-state.md)                       | 編集状態をコアとクライアントに分ける                                 | 採用（アンドゥは 0004 で置き換え） |
| [0004](./0004-undo-in-core.md)                       | アンドゥの履歴はコアが持つ                                           | 採用                               |
| [0005](./0005-native-plugin-ui.md)                   | プラグイン UI はネイティブのウィジェット木で作る                     | 採用                               |
| [0006](./0006-user-controlled-ui-and-slots.md)       | ユーザー制御 UI とスロットによるステータスライン                     | 採用                               |
| [0010](./0010-remote-connection.md)                  | SSH で接続し、再起動と再同期でセッションを戻す                       | 採用                               |
| [0011](./0011-backend-plugins-on-core.md)            | バックエンドプラグインはコアで動かす                                 | 採用（実装は Phase 6）             |
| [0017](./0017-cli-parsing-with-usage-rs.md)          | CLI 引数の解析に usage-rs を使う                                     | 採用                               |
| [0018](./0018-typed-rpc.md)                          | クライアント・コア間の RPC は型付き marker で送る                    | 採用                               |
| [0019](./0019-global-vy-api.md)                      | プラグイン API はグローバル `vy` に統一する                          | 採用                               |
| [0020](./0020-plugin-source-and-bundles.md)          | プラグインはソース形態とバンドル形態の2つを許す                      | 採用                               |
| [0021](./0021-bundle-validation.md)                  | バンドルとエントリをロード時に検証する                               | 採用                               |
| [0022](./0022-command-surfaces.md)                   | コマンドの入口を役割で分け、設定を状態として分離する                 | 採用                               |
| [0024](./0024-keyboard-first-plugin-ui.md)           | キーボードファーストなプラグイン UI                                  | 採用（実装は未着手）               |
| [0025](./0025-plugin-api-declaration-and-codegen.md) | プラグイン API の宣言・型・生成を名前空間 trait と Specta に集約する | 採用（実装済み）                   |

### 置き換えられた ADR

[`superseded/`](./superseded/README.md) に 9 本（0007, 0008, 0009, 0012, 0013, 0014, 0015, 0016, 0023）を保管しています。決定の内容は 0025 に引き継いでおり、番号は履歴として残します。
