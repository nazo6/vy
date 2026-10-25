---
title: "通信プロトコル仕様 (MessagePack RPC)"
type: "spec"
status: "active"
updated_at: 2026-10-25
related_adrs: ["0002", "0010", "0018"]
tags: ["protocol", "rpc", "msgpack", "network"]
---

# 通信プロトコル仕様 (MessagePack RPC)

Vy
のローカルクライアント（GPUIフロントエンド）とエディタサーバー（ヘッドレスコア）の間は、**Pure
RustのMessagePack (`rmp-serde`)** を用いた高速バイナリRPCで通信を行います。

---

## 1. シリアライズ方式: Pure Rust MessagePack

- **採用クレート**: [`rmp-serde`](https://github.com/3Hren/msgpack-rust)
- **選定理由**:
  - **C言語依存ゼロ (Pure Rust)**: クロスコンパイルやセキュリティ監査が容易。
  - **コンパクト & 高速**:
    JSONと比較してパケットサイズを30〜60%削減し、シリアライズ/デシリアライズCPU時間を大幅短縮。
  - **動的型との親和性**:
    TypeScriptプラグイン層（QuickJS）とも型安全かつ自然にマッピング可能。

ローカル接続では、クライアントが `vy-server`
を子プロセスで起動し、標準入出力でこのフレームを流します。リモート接続では、SSH
の標準入出力を使います。どちらも同じ形式です（[ADR 0002](../adr/0002-process-model.md)）。

リモート接続は `ssh -T [-p port] [user@]host -- <serverPath> --stdio` で起動します。
接続先には導入済みの `vy-server` を使い、バイナリの自動転送は行いません
（[ADR 0010](../adr/0010-remote-connection.md)）。接続先は
`~/.config/vy/connections.json` のプロファイルで選びます。

`vy-server` を起動するときの引数は `--stdio`（既定）と `--version`
だけにします。ワークスペース、ログの出力先、再開するセッションは引数ではなく、§3
の `initialize` の params で渡します。接続先が同じでも、引数と params
の2か所で状態を決めると、再接続のときにどちらが正か分からなくなるためです。

---

## 2. メッセージエンベロープ仕様

すべてのRPCフレームは、以下のバイナリヘッダーとMessagePackペイロードで構成されます。

```text
+-----------------------+-------------------------+
| Length (4 bytes, u32) | MessagePack Payload ... |
+-----------------------+-------------------------+
```

- 長さはリトルエンディアンの u32
  とし、ペイロードの大きさだけを含めます。ヘッダーは含めません。
- 1つのフレームの上限は 64 MiB です。超えるデータは §5 の方法で分割します。
- `buffer/snapshot`
  は本文を1つのフレームで送るため、上限は全体の本文が収まる大きさにしています。分割送信を実装したら下げます。
- 読み手と書き手は、それぞれ1本のスレッドまたはタスクに固定します。同じパイプに複数の書き手を作りません。

### ペイロードの基本構造 (Rust Enum)

```rust
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum RpcMessage {
    /// クライアント -> サーバーへのリクエスト
    Request {
        id: u64,
        session_id: u64,
        method: String,
        params: rmpv::Value,
        deadline_ms: Option<u32>,
    },
    /// サーバー -> クライアントへの応答
    Response {
        id: u64,
        result: RpcResult,
    },
    /// 実行中のリクエストの取り消し
    Cancel {
        id: u64,
        reason: Option<String>,
    },
    /// 双方向の一方向通知（イベント / 差分更新）
    Notification {
        method: String,
        params: rmpv::Value,
    },
    /// 連続するデータの断片
    Stream {
        stream_id: u64,
        seq: u64,
        payload: StreamPayload,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum RpcResult {
    Ok { value: rmpv::Value },
    Err { code: ErrorCode, message: String, data: Option<rmpv::Value> },
}

#[derive(Serialize, Deserialize, Debug)]
pub enum StreamPayload {
    Chunk(rmpv::Value),
    End { summary: Option<rmpv::Value> },
    Error { code: ErrorCode, message: String },
}
```

### 実装上の型

エンベロープのメソッド名と `params` / 応答は、実装では `vy-proto` の
`RpcMethod` / `RpcNotification` で結びつけます。ワイヤーの形は上のままで、
メソッド名の文字列と型の対応を1か所に置くだけです。

```rust
client.request::<PaneOpen>(PaneOpenParams { .. });
client.notify::<BufferDidChange>(DidChangeParams { .. });
```

サーバーは `PaneOpen::METHOD` で分岐し、`decode_params` と `ok` で型付きの
params / 応答を扱います。詳細は [ADR 0018](../adr/0018-typed-rpc.md) にあります。

## 3. 接続の開始

接続したら、クライアントは `initialize` を送ります。サーバーは `initialized`
を通知として返し、セッション ID を渡します。

```text
Request {
    id: 1,
    session_id: 0,
    method: "initialize",
    params: {
        "protocol_version": 3,
        "client_version": "0.1.0",
        "client_capabilities": ["stream", "session-restore"],
        "workspace": "file:///home/user/project",
        "resume_session": null,
    }
}
```

```text
Notification {
    method: "initialized",
    params: {
        "protocol_version": 3,
        "session_id": 7,
        "server_version": "0.1.0",
        "server_capabilities": ["stream", "session-restore"],
        "platform": { "os": "linux", "path_separator": "/" },
    }
}
```

版が合わない場合は `UnsupportedProtocolVersion`
で応答し、接続を閉じます。クライアントは `server_capabilities`
にない機能を呼びません。現在の主な capability は次のとおりです。

| capability        | 意味                                        |
| :---------------- | :------------------------------------------ |
| `stream`          | `Stream` で分割したデータを送れる           |
| `session-restore` | `sync/request` と `layout/get` で再開できる |
| `plugin`          | `plugin/*` を受け付けられる（Phase 6）      |

`resume_session` は Phase 5 では使いません。接続が切れたら新しいセッションを作り、
クライアントが `layout/get` と `sync/request` で状態を戻します。

1つのサーバーは複数のセッションを持てます。ウィンドウごとかワークスペースごとにセッションを作り、すべての
`Request` に `session_id` を付けます。`initialize` の `session_id` だけは 0
です。

## 4. 取り消しと期限

`deadline_ms` は、応答を待つ上限です。過ぎた場合はサーバーが `Timeout`
で応答し、処理を中断します。

`Cancel`
は、実行中のリクエストを止める通知です。サーバーは可能な範囲で止め、`Cancelled`
で応答します。すでに完了している場合は無視します。

ストリームを伴うリクエストを取り消した場合は、`Stream` の `Error` に `Cancelled`
を入れて終了します。

## 5. ストリームと大容量データ

検索結果やプロセスの出力は、`Stream` で分割して送ります。`seq` は 0
から始まる連番で、抜けと順序の入れ替わりを検出するために使います。

受信側は、処理が追いつかないときに `flow/ack` を返します。送信側は ack
が来るまで次のチャンクを送りません。これを省略すると、受信側のキューが伸び続けます。

1つのメッセージの上限は 64 MiB とします。超えるデータは次のどちらかで渡します。

- ストリームに分割する。
- サーバーに一時ファイルを作り、その URI を `Blob` として渡す。

`Blob` は `blob/create`、`blob/read`、`blob/release`
の3つのメソッドで扱います。クライアントは内容を読まずに URI
として扱えます。画像や大きな JSON に使います。

## 6. エラー

`ErrorCode` は次の値を持ちます。

| 値                           | 意味                                   |
| :--------------------------- | :------------------------------------- |
| `InvalidRequest`             | メソッド名が不正                       |
| `InvalidParams`              | 引数の形か型が不正                     |
| `CapabilityDenied`           | 宣言のないケイパビリティを使おうとした |
| `NotFound`                   | 対象の文書、ペイン、プラグインがない   |
| `InvalidState`               | 現在の状態では実行できない             |
| `Cancelled`                  | 取り消された                           |
| `Timeout`                    | 期限を過ぎた                           |
| `UnsupportedProtocolVersion` | プロトコルの版が合わない               |
| `UnsupportedFeature`         | 相手が対応していない機能を呼んだ       |
| `ResourceLimit`              | メモリか実行時間の上限を超えた         |
| `Internal`                   | サーバー内部の失敗                     |

`data`
には、利用者に見せられる詳細を入れます。スタックトレースは入れません。ログにだけ残します。

## 7. 順序と再接続

同じ文書に対する `Notification` は、`version`
の昇順で送ります。別の文書の通知との順序は保証しません。

接続が切れた場合、クライアントは自動で再接続します。Phase 5 では `resume_session`
を使わず、再接続を新しいセッションとして扱います。サーバーは `initialize` の
`workspace` から保存済みのレイアウトを読み、開いていた文書をディスクから開きます。

1. クライアントは `initialize` を送り、`initialized` を受ける。
2. クライアントは `layout/get` を送り、復元されたペインと document ID を受け取る。
3. クライアントは開いている文書を `sync/request` で送る。未保存の文書だけ本文を含める。
4. サーバーは各文書の全文と version を返す。クライアントは表示と version を合わせる。

document ID と pane ID は接続ごとに変わり得ます。クライアントは URI で対応づけます。
購読はセッションに紐づくため、作り直しが必要です。

再接続のあいだ、クライアントは落ちません。自動再接続のほかに、利用者が
`vy.reconnect_server` を実行して手動で試すこともできます。

## 8. ペインとレイアウト

ペインのレイアウトはコアが持ちます。メッセージの種類、`layout/did_change`
の形、`revision` の扱いは [`panes.md`](./panes.md)
の「メッセージ」にまとめました。

クライアントは操作の意図を `pane/open` などで送り、確定した木を
`layout/did_change` で受け取ります。受け取った `revision`
が手元より新しいときだけ適用します。

`layout/get` の `panes` には、ペインごとに `pane_id`、`view`、`document_id`、
`uri` が入ります。再接続では、クライアントが `uri` で手元の文書と対応づけます。
ファイルが見つからないペインは `document_id` を省略し、空のエディタとして描きます。

## 9. 主要なRPCメソッド例

### (A) バッファ差分通知 (`buffer/did_change`)

キー入力による変更を、クライアントが適用したあとに送ります。`base_version`
は、変更を適用する前の version です。`selection_before` と `selection_after`
は、アンドゥでカーソルを戻すためにコアが保存します。

```rust
Notification {
    method: "buffer/did_change",
    params: {
        "buffer_id": 1,
        "base_version": 41,
        "version": 42,
        "transaction_id": 901,
        "edits": [
            { "range": { "start": [10, 5], "end": [10, 5] }, "text": "let x = 1;" }
        ],
        "selection_before": [
            { "anchor": [10, 5], "head": [10, 5] }
        ],
        "selection_after": [
            { "anchor": [10, 5], "head": [10, 15] }
        ],
    }
}
```

同じ `transaction_id`
を続けて送ると、コアは1つのアンドゥ項目にまとめます。入力を続けている間は同じ ID
を使い、次のどれかで新しい ID にします。

- モードが変わったとき
- 最後の編集から 500ms 以上たったとき
- 選択を伴うコマンドを実行したとき

コアが起こした編集（整形、リネーム、複数ファイルの置換）は、呼び出し1回を1つのアンドゥ項目にします。

サーバーは `base_version` が手元と合わない場合、変更を受け付けずに
`buffer/resync` を送ります。クライアントはその文書を読み直します。

### (B) リモートプロセス実行 (`process/spawn`)

フロントエンドJSから要求されたコマンドを、接続先で起動します。

```rust
Request {
    id: 101,
    session_id: 7,
    method: "process/spawn",
    params: {
        "command": "rg",
        "args": ["--line-number", "main"],
        "cwd": "file:///home/user/project",
        "capability_token": "example.hello@0.1.0",
    },
    deadline_ms: null,
}
```

標準出力と標準エラーは `stream_id` を分けて送ります。終了コードは `onExit`
に対応する `process/exited` の通知で返します。

### (C) 検索 (`workspace/grep`)

```rust
Request {
    id: 102,
    session_id: 7,
    method: "workspace/grep",
    params: {
        "query": "fn main",
        "include": "**/*.rs",
        "max_results": 1000,
    },
    deadline_ms: 30000,
}
```

結果は `stream_id` ごとに `Chunk` で届き、最後に `End` が届きます。`max_results`
に達した場合は `End` の `summary` に `truncated: true` を入れます。

### (D) アンドゥ (`buffer/undo`)

```rust
Request {
    id: 103,
    session_id: 7,
    method: "buffer/undo",
    params: { "buffer_id": 1, "pane_id": 4 },
    deadline_ms: null,
}
```

応答には、適用した逆編集と、戻したあとの選択が入ります。クライアントは本文と選択を同時に適用します。`buffer/redo`
も同じ形です。`pane_id` は、戻したあとにフォーカスを移す先です。

プロトコル版 2 以降では、`edits` は編集バッチの配列です。同じ `transaction_id`
をまとめたアンドゥでは複数のバッチが返り、各バッチは直前のバッチを適用したあとの座標を使います。クライアントは配列の順に適用し、version
は最後に 1 回だけ進めます。バッチを 1
つに連結すると座標系が混ざり、重複や範囲外の位置になるためです。

### (E) 再接続の同期 (`sync/request`)

再接続したクライアントが、開いている文書を送ります。未保存の文書だけ `text` を含めます。
保存済みの文書は `text` を省略し、サーバーがディスクから読み直します。

```rust
Request {
    id: 104,
    session_id: 8,
    method: "sync/request",
    params: {
        "documents": [
            {
                "uri": "file:///home/user/project/src/main.rs",
                "version": 42,
                "saved_version": 40,
                "text": "fn main() {}\n",
            },
            {
                "uri": "file:///home/user/project/README.md",
                "version": 0,
                "saved_version": 0,
            },
        ],
    },
    deadline_ms: 30000,
}
```

応答には、文書ごとの `document_id`、`uri`、`version`、`saved_version`、`text`
が入ります。サーバーはアンドゥ履歴を捨ててバッファを作り直します。クライアントは
返ってきた全文と version を表示に反映します。
