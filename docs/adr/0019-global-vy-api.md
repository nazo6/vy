# ADR 0019: プラグイン API はグローバル `vy` に統一する

- 状態: 採用（実装はこれから）
- 日付: 2026-10-24

## 文脈

現在のプラグイン API は `import { editor, commands } from "vy-api"` で読み込みます。
`vy-api` は QuickJS の loader が返す仮想モジュールで、ファイルとしては存在しません。
一方で、対話評価のために `globalThis.vy` も用意しており、入口が2つあります。

この形には次のコストがあります。

- `vy-api` を解決するために resolver と loader に特別扱いが必要です。
- バンドルするプラグインは、どのツールチェーンでも `vy-api` を external に
  指定する必要があります。
- ソース形態のプラグインでも import 文が必要で、モジュール解決の対象になります。
- 対話評価とプラグインで入口が異なります。

[ADR 0020](./0020-plugin-source-and-bundles.md) で「ソース形態は相対 import だけ、
外部依存はバンドル」と決めるため、`vy-api` の特別扱いを残す理由が薄くなりました。

## 決定ドライバ

1. 実行時の入口を1つにすること。
2. Vy 固有のバンドル設定を不要にすること。
3. ソース形態のプラグインを import なしで書けること。
4. 型の補完と `CommandMap` の拡張を維持すること。
5. プラグインごとのランタイム分離と両立すること。

## 検討した選択肢

| 選択肢 | 内容 | 評価 |
| :--- | :--- | :--- |
| A. 現状維持 | `vy-api` モジュールだけを使う | resolver とバンドル設定の特別扱いが残る |
| B. 両方を残す | モジュールとグローバルを併存させる | 入口が2つになり、ドキュメントと実装が二重になる |
| C. グローバルのみ（採用） | ホストが `vy` をインストールする | 入口が1つになり、バンドル設定が不要になる |

## 決定

### 1. 実行時の入口

- プラグインと `init.ts` は `vy` グローバルを使います。
- `vy-api` 仮想モジュールは廃止します。
- ホストはスクリプトを評価する前に `globalThis.vy` をインストールします。
- shim は ESM の export ではなく、`globalThis.vy` を代入するスクリプトとして
  生成します。

```typescript
// init.ts
vy.config.set({ values: { "ui.fontSize": 14 } });

vy.commands.registerCommand("user.greet", async () => {
  await vy.notifications.show({ message: "Hello" });
});
```

### 2. 型

- `packages/vy-api` は型専用のパッケージとして残します。
- `index.d.ts` は ambient 宣言を持ちます。既存の `types/`、
  `api-base.d.ts`、`shim.d.ts` の export は残し、ambient 宣言を足します。

```typescript
import type { VyApi } from "./api-base";

declare global {
  const vy: VyApi;
  namespace Vy {
    interface CommandMap {}
  }
}

export {};
```

- 利用側は型を1回読み込みます。テンプレートでは `env.d.ts` に
  `import type {} from "vy-api";` を置きます。`init.ts` でも同じ行を置くか、
  隣に `init-env.d.ts` を置きます。
- `tsconfig` の `types` は既定で `node_modules/@types` を見るため、通常の
  パッケージでは届かないことがあります。`import type {}` を1回置く方法は、
  この解決に依存しません。

### 3. `CommandMap` の拡張

- `declare global { namespace Vy { interface CommandMap {} } }` を拡張点にします。
- 利用者は `declare global` で追加します。`init.ts` のように import のない
  ファイルでは、`export {}` を足してモジュールにします。

```typescript
declare global {
  namespace Vy {
    interface CommandMap {
      "user.greet": { name: string };
    }
  }
}
export {};
```

### 4. 分離

- `vy` は QuickJS のランタイムごとに1つです。プラグインごとにランタイムを
  分けるため、別のプラグインの `vy` は見えません。
- 共有ランタイムに戻すと `vy` も共有されるため、ランタイム分離は前提条件です。

### 5. backend

- backend の型は今は frontend と同じ1つにします。
- UI 系の API も型には出ますが、実行時はコアのケイパビリティ検査で拒否します。
- backend 用に型を絞るのは Phase 6 に見送ります。

## 帰結

### 良い面

- resolver の特別扱いが消え、相対ファイルの解決だけになります。
- バンドルで `external` の指定が不要になります。
- ソース形態のプラグインは import なしで書けます。
- 対話評価とプラグインの入口が同じになります。
- ランタイムごとに `vy` が分かれ、ケイパビリティと対応します。

### 悪い面・課題

- 依存がコードから見えにくくなります。
- 型を有効にするために、1回の型 import か env ファイルが必要です。
- `CommandMap` の拡張に `declare global` と、必要に応じて `export {}` が要ります。
- backend の型に UI API が出ます。
- 既存のプラグインとドキュメントの書き換えが必要です。

### 緩和策

- テンプレートに `env.d.ts` を同梱します。
- backend の型の分割は Phase 6 の課題として roadmap に残します。
- 型の読み込み方法はテンプレートとドキュメントで1つに揃えます。

## 見直しの条件

1. 複数の API バージョンを1つのランタイムで扱いたくなったとき。
2. backend の型を分ける必要が出たとき（Phase 6）。
3. プラグイン作者が import による明示的な依存を強く求めたとき。

## 関連する未決定事項

- 型パッケージの名前を `vy-api` のままにするか、`@vy/types` などに変えるか。
- `vy` のプロパティを freeze するか。
- `CommandMap` の拡張を `Vy.CommandMap` にするか、別名にするか
  （今は `Vy.CommandMap`）。
- 型の読み込みを `import type {}` に統一するか、`types` と `typeRoots` で
  解決するか。
