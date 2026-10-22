# 通信プロトコル仕様 (Client-Server RPC)

Vy のローカルクライアント（TUI/GUI）とエディタサーバー（ヘッドレスコア）の間は、双方向RPCプロトコルで通信を行います。

---

## 1. 接続トランスポート

1. **ローカル単体起動時**:
   - プロセス内チャネル（Tokio MPSC channel）または Unix Domain Socket。
2. **リモート開発接続時**:
   - **SSH stdio**: SSHコマンド経由でリモートの `vy server --stdio` を起動し、標準入出力でバイナリストリーム通信。
   - **WebSocket / TLS**: 常駐デーモンとして稼働しているリモートVyサーバーへのTCP接続。

---

## 2. メッセージ種別

プロトコルは基本的に **JSON-RPC 2.0 互換**（またはバイナリシリアライズ）の形式をとります。

### (A) Request / Response (同期的・非同期的な要求)
```json
// Client -> Server: ファイルオープン要求
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "buffer/open",
  "params": {
    "path": "/home/user/project/src/main.rs"
  }
}

// Server -> Client: バッファ内容の初期同期
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "buffer_id": 42,
    "version": 1,
    "lines": ["fn main() {", "    println!(\"Hello, Vy!\");", "}"]
  }
}
```

### (B) Notification (一方向の通知・イベント)
```json
// Client -> Server: キー入力による確定差分（ローカル予測適用後）
{
  "jsonrpc": "2.0",
  "method": "buffer/didChange",
  "params": {
    "buffer_id": 42,
    "version": 2,
    "changes": [
      { "range": { "start": [1, 27], "end": [1, 27] }, "text": "\n" }
    ]
  }
}

// Server -> Client: LSP診断エラーのプッシュ通知
{
  "jsonrpc": "2.0",
  "method": "diagnostics/publish",
  "params": {
    "buffer_id": 42,
    "diagnostics": [
      { "line": 1, "severity": "error", "message": "cannot find function" }
    ]
  }
}
```

---

## 3. これから決めること (TODO / 未決定事項)

- [ ] **シリアライゼーション形式の確定**
  - デバッグやWeb親和性に優れた **JSON-RPC** をベースにするか、
  - リモートでの転送量削減・シリアライズ高速化のために **MessagePack** や **FlatBuffers / Cap'n Proto** などのゼロコピーバイナリ形式を採用するか。
- [ ] **差分同期アルゴリズムの選定**
  - 単純なバージョン番号管理 + テキスト範囲差分（LSP準拠）か、
  - 一時的なオフラインや回線断からの復帰に強いCRDT / Operational Transformationベースの差分プロトコルにするか。
- [ ] **認証と暗号化 (Daemon起動時)**
  - SSHトンネルを前提とすればSSH層に認証・暗号化を委任可能だが、スタンドアローンデーモン時のトークン認証やTLS証明書仕様の策定。
