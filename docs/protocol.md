# 通信プロトコル仕様 (MessagePack RPC)

Vy のローカルクライアント（GPUIフロントエンド）とエディタサーバー（ヘッドレスコア）の間は、**Pure RustのMessagePack (`rmp-serde`)** を用いた高速バイナリRPCで通信を行います。

---

## 1. シリアライズ方式: Pure Rust MessagePack

- **採用クレート**: [`rmp-serde`](https://github.com/3Hren/msgpack-rust)
- **選定理由**:
  - **C言語依存ゼロ (Pure Rust)**: クロスコンパイルやセキュリティ監査が容易。
  - **コンパクト & 高速**: JSONと比較してパケットサイズを30〜60%削減し、シリアライズ/デシリアライズCPU時間を大幅短縮。
  - **動的型との親和性**: TypeScriptプラグイン層（QuickJS）とも型安全かつ自然にマッピング可能。

---

## 2. メッセージエンベロープ仕様

すべてのRPCフレームは、以下のバイナリヘッダーとMessagePackペイロードで構成されます。

```text
+-----------------------+-------------------------+
| Length (4 bytes, u32) | MessagePack Payload ... |
+-----------------------+-------------------------+
```

### ペイロードの基本構造 (Rust Enum)
```rust
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum RpcMessage {
    /// クライアント -> サーバーへのリクエスト
    Request {
        id: u64,
        method: String,
        params: rmpv::Value,
    },
    /// サーバー -> クライアントへの応答
    Response {
        id: u64,
        result: Result<rmpv::Value, RpcError>,
    },
    /// 双方向の一方向通知（イベント / 差分更新）
    Notification {
        method: String,
        params: rmpv::Value,
    },
}
```

---

## 3. 主要なRPCメソッド例

### (A) バッファ差分通知 (`buffer/did_change`)
キー入力による変更差分を送信（ローカル側で即時反映された後の確定イベント）。
```rust
Notification {
    method: "buffer/did_change",
    params: {
        "buffer_id": 1,
        "version": 42,
        "range": { "start": [10, 5], "end": [10, 5] },
        "text": "let x = 1;",
    }
}
```

### (B) リモートプロセス実行 (`process/spawn`)
フロントエンドJSから要求されたコマンドをリモートサーバー上で起動。
```rust
Request {
    id: 101,
    method: "process/spawn",
    params: {
        "command": "rg",
        "args": ["--line-number", "main"],
        "cwd": "/home/user/project",
    }
}
```
