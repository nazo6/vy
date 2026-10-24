# ADR 0018: クライアント・コア間の RPC は型付き marker で送る

- 状態: 採用
- 日付: 2026-10-24

## 文脈

クライアントとコアの間の RPC は、エンベロープではメソッド名を文字列、
params と応答を `rmpv::Value` で運びます（[`protocol.md`](../protocol.md)）。
この形はワイヤーとしては単純ですが、実装では次の問題がありました。

- 呼び出し側が `client.request("pane/open", to_value(&PaneOpenParams { .. }))`
  のように、メソッド名と params 型を別々に書く。
- 応答は `Pending` の種類を見て、呼び出し側が
  `decode_value::<PaneOpenResult>` する。
- サーバー側は `match method { "pane/open" => .. }` の文字列分岐に加え、
  `get(&params, "pane_id")` でフィールドを手で読む箇所がある。
- プラグインの `fs` / `process` 実装は、host API の params を
  `vy_proto` の params に詰め替え、メソッド名も文字列で組み立てる。

メソッド名・params・応答の対応が間違っていてもコンパイルは通り、実行時に
デコードできずに状態行へエラーが出るだけでした。

また、`vy-proto` と `vy-host-api` / `vy-client-api` に同じような params 型が
あります。これはクライアント・コア間の内部プロトコル（MessagePack、
snake_case、`pane/open`）と、プラグイン向けの公開 API（JSON、camelCase、
`fs.readFile`）が別の契約だからです。

## 決定ドライバ

1. メソッド名と params / 応答型の対応をコンパイル時に検査すること。
2. ワイヤー形式を変えず、既存のクライアントとサーバーを同時に更新できること。
3. プラグイン向け公開 API の命名と互換性を変えないこと。
4. メソッドを追加したときに、追加漏れを型で検出できること。

## 検討した選択肢

| 選択肢                              | 内容                                                               | 評価                                             |
| :---------------------------------- | :----------------------------------------------------------------- | :----------------------------------------------- |
| A. 文字列のまま                     | 現状を維持する                                                     | 変更は小さいが、型のずれを実行時まで検出できない |
| B. 型付き marker（採用）            | `RpcMethod` にメソッド名・params・応答を持たせる                    | ワイヤーを変えず、呼び出しと分岐を型で結べる     |
| C. protocol と host API の型を統合  | 同じ struct を両方で使う                                           | 命名か公開 API の互換性を犠牲にする              |
| D. スキーマからコード生成           | 宣言からクライアント・サーバーのコードを生成する                   | 将来は有力だが、今の変更としては大きい           |

## 決定

### 1. `vy-proto` に型付きの marker を置く

`vy-proto::rpc` に次の trait と marker 型を置きます。

```rust
pub trait RpcMethod {
    const METHOD: &'static str;
    type Params: Serialize + DeserializeOwned;
    type Reply: Serialize + DeserializeOwned;
}

pub struct PaneOpen;
impl RpcMethod for PaneOpen {
    const METHOD: &'static str = "pane/open";
    type Params = PaneOpenParams;
    type Reply = PaneOpenResult;
}
```

通知も `RpcNotification` で同じように宣言します。marker は
`initialize`、`pane/*`、`layout/get`、`editor/open`、`document/save`、
`buffer/*`、`sync/request`、`fs/*`、`process/*`、`initialized`、
`buffer/resync`、`layout/did_change` を対象にします。

### 2. クライアントは marker を指定して送る

```rust
client.request::<PaneOpen>(PaneOpenParams { .. });
client.notify::<BufferDidChange>(DidChangeParams { .. });
```

応答は送信時と同じ marker でデコードします。

```rust
let opened = decode_reply::<PaneOpen>(&value);
```

### 3. サーバーは marker のメソッド名で分岐する

`Session::request` は `PaneOpen::METHOD` のような定数で分岐し、
`decode_params::<M>` で params を受けて、`ok::<M>` で応答します。文字列の
メソッド名と `get(&params, ..)` によるフィールド読み取りは削除します。

### 4. 動的な境界では `RpcCall` を使う

プラグインの `ServerRpc` のように、呼び出し側が generic なメソッド型を
名指しできない場所では、params が分かっている時点で
`RpcCall::new::<FsReadFile>(params)` を作ります。メソッド名と params は
この値の中で結びついたままクライアントへ渡ります。

### 5. 型の共通化はしない

`vy-proto` と `vy-host-api` / `vy-client-api` の params / reply 型は統合しません。
両者は次の点で異なる契約です。

| | `vy-proto` | host API |
| :--- | :--- | :--- |
| 境界 | クライアント ↔ コア（内部） | プラグイン ↔ ホスト（公開） |
| 形式 | MessagePack | JSON |
| フィールド名 | snake_case | camelCase |
| 互換性 | プロトコル版で管理 | TypeScript の公開型として管理 |

`DirEntry`、`FileStat`、`ProcessOutput` のような境界に依存しない値型は、
現在どおり `vy-proto` に置いて host API から再利用します。params / reply は
各境界の型として残し、必要なら `From` で変換します。

## 帰結

### 良い面

- メソッド名・params・応答の対応がコンパイル時に検査される。
- サーバーの文字列分岐と手動のフィールド読み取りが減る。
- `fs` / `process` のプラグイン実装が、メソッド名を文字列で組み立てない。
- ワイヤー形式と生成 TypeScript は変わらない。

### 悪い面

- メソッドを追加するときに marker と実装の両方を更新する。
- `Pending` と marker の対応はまだ `EditorView` 側に残る。
- 動的な `RpcCall` はメソッド名と params の対応を型では保証しない
  （構築時点では保証される）。

### 緩和策

- marker の `METHOD` を使ってサーバーの分岐を書き、文字列の重複を避ける。
- `fs` / `process` の `RpcCall` は `RpcCall::new::<M>` だけを使い、
  `rmpv::Value` を直接組み立てない。
- 将来、`Pending` も応答型を持つ形に置き換える。

## 見直しの条件

1. `Pending` の対応付けが増え、marker とのずれが問題になったとき。
2. メソッドの宣言からクライアント・サーバーのコードを生成したくなったとき。
3. host API とプロトコルの命名を揃える必要が出たとき（プロトコル版の更新が必要）。

## 関連する未決定事項

- `Pending` を型付きの応答ハンドラに置き換えるか。
- protocol と host API で共有する値型を、`vy-proto` 以外の層へ移すか。
- `RpcMethod` の一覧からサーバーの分岐を生成するか。
