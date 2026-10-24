# ADR 0013: 名前空間 trait と `HostApi` facade でホスト API を実装する

- 状態: 採用（試作: `workspace` と `notifications`）
- 日付: 2026-10-24

## 文脈

ADR 0012 では、API の型と宣言を `vy-host-api` / `vy-client-api` に置き、
ハンドラを `vy-app` / `vy-server` に置く形にしました。この形は生成物を変えずに
移行できましたが、ハンドラごとに adapter と `HandlerDecl` を並べる必要があり、
「実装するだけ」にはなっていませんでした。

また、すべてのメソッドを1つの大きな `HostApi` trait に入れると、次の問題があります。

- `views.open` と `editor.open` のように Rust のメソッド名が衝突する
- クライアント専用のメソッドをコア側も実装するか、`#[cfg]` で分ける必要がある
- 1メソッド追加の影響が巨大な trait ファイルに集中する

## 決定

### 1. 名前空間ごとに trait を宣言する

`vy-host-api` と `vy-client-api` の名前空間モジュールに trait を置きます。

```rust
#[vy_api_trait("workspace")]
pub trait WorkspaceApi {
    #[vy_api_method]
    fn info(&mut self) -> Result<Reply<WorkspaceInfoReply>, CallError>;

    #[vy_api_method]
    fn text_documents(&mut self) -> Result<Reply<Vec<DocumentInfoReply>>, CallError>;
}
```

`#[vy_api_trait]` は trait を残したまま、次を生成します。

- メソッドごとの `MethodDecl`（ワイヤー名、params / reply 型、JSDoc）
- `METHODS`
- `NAMESPACE`
- `dispatch<H: Trait + ?Sized>(host, method, params)`

`parse_params`、`erase_reply`、`Reply`、`CallError` は `vy-host-api` に置きます。

### 2. facade trait で dispatch を1か所にする

共有側は `vy-host-api` の `HostApi` が名前空間 trait を束ねます。

```rust
pub trait HostApi: workspace::WorkspaceApi {
    fn dispatch(&mut self, method: &str, params: &Value)
        -> Option<Result<Reply<Value>, CallError>>;
}
```

クライアント側は `vy-client-api` の `ClientHostApi` が `HostApi` を拡張します。

```rust
pub trait ClientHostApi:
    vy_host_api::HostApi + notifications::NotificationsApi
{
    fn dispatch(&mut self, method: &str, params: &Value)
        -> Option<Result<Reply<Value>, CallError>>;
}
```

`dispatch` は default method にし、名前空間の dispatch を順に呼びます。
実装側は名前空間 trait を実装するだけで、メソッドごとの adapter は書きません。

### 3. ホスト構造体に実装する

```rust
pub struct ClientHost<'a, 'b, 'c> {
    pub call: &'a mut Call<'b, 'c>,
}

impl WorkspaceApi for ClientHost<'_, '_, '_> { ... }
impl NotificationsApi for ClientHost<'_, '_, '_> { ... }
impl vy_host_api::HostApi for ClientHost<'_, '_, '_> {}
impl ClientHostApi for ClientHost<'_, '_, '_> {}
```

`vy-app` の dispatch は `ClientHostApi::dispatch` を呼び、残りの
宣言ベースの名前空間は従来の registry にフォールバックします。

### 4. イベントは trait の外に置く

イベントはハンドラではないため、`event_decl!` と `EVENTS` で宣言します。
trait マクロはメソッドのメタデータだけを生成します。

## 今回の試作

- `workspace` を `vy-host-api` の trait に変更
- `notifications` を `vy-client-api` の trait に変更
- `HostApi` と `ClientHostApi` を追加
- `Reply`、`CallError`、`parse_params`、`erase_reply` を `vy-host-api` に移動
- `vy-app` に `ClientHost` を追加し、`workspace` と `notifications` を実装
- 生成される TypeScript は変えない

`fs`、`process`、`document`、`config`、`editor`、`views` などは、試作の結果を見て
同じ形に移します。

## 帰結

### 良い面

- `vy-app` / `vy-server` は trait の実装に集中できる
- メソッドごとの adapter と宣言の重複がない
- メソッド追加時に trait の実装漏れをコンパイル時に検出できる
- クライアント専用と共有を trait の分離で表せる
- `dispatch` の入口が1つになる

### 悪い面

- ホスト構造体と facade trait が必要になる
- trait にメソッドを追加すると、その trait を実装する全ホストが影響を受ける
- 名前空間を facade に追加するときにマクロのリストを更新する必要がある

### 緩和策

- 名前空間ごとに trait を分け、メソッド追加の影響をその名前空間に閉じる
- `HostApi` は共有、`ClientHostApi` は共有 + クライアント専用と分ける
- 試作で `workspace` と `notifications` を通し、移行コストを確認する

## 見直しの条件

1. facade trait の dispatch が大きくなりすぎる。
2. ホスト構造体のライフタイムや可変借用が扱いにくくなる。
3. trait の実装漏れよりも、宣言ベースの柔軟さが欲しくなる。

## 関連する未決定事項

- `fs` や `process` の `Reply::Deferred` を trait でどう扱うか。
- `config` の共有 trait とクライアント trait の分割方法。
- `vy-api-macros` を facade の生成まで広げるか。
