# ADR 0013: 名前空間 trait と `HostApi` facade でホスト API を実装する

- 状態: 採用（すべての名前空間を trait 化）
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
- イベントごとの `EventDecl` と `EVENTS`
- `NAMESPACE`
- `dispatch<H: Trait + ?Sized>(host, method, params)`

`parse_params`、`erase_reply`、`Reply`、`CallError` は `vy-host-api` に置きます。

### 2. facade trait で dispatch を1か所にする

`host_api!` が facade trait、dispatch、コード生成用の関数をまとめて生成します。

```rust
host_api! {
    trait HostApi: HostRoot {
        base_methods_fn = host_root_methods;
        base_namespaces_fn = host_root_namespaces;
        base_events_fn = host_root_events;
        traits = [
            config => ConfigReadApi,
            fs => FsApi,
            workspace => WorkspaceApi,
        ];
        methods_fn = host_api_methods;
        namespaces_fn = host_api_namespaces;
        events_fn = host_api_events;
    }
}
```

クライアント側は `HostApi` を base にして、クライアント専用の trait を足します。

```rust
host_api! {
    trait ClientHostApi: vy_host_api::HostApi {
        base_methods_fn = vy_host_api::host_api_methods;
        base_namespaces_fn = vy_host_api::host_api_namespaces;
        base_events_fn = vy_host_api::host_api_events;
        traits = [
            commands => CommandsApi,
            config => ConfigApi,
            notifications => NotificationsApi,
            views => ViewsApi,
        ];
        methods_fn = client_host_api_methods;
        namespaces_fn = client_host_api_namespaces;
        events_fn = client_host_api_events;
    }
}
```

生成されるもの:

- facade trait の supertrait と `dispatch`
- `host_api_methods()` / `host_api_namespaces()` / `host_api_events()`
- クライアント側は base の関数にクライアント専用の宣言を足す

同じ名前空間が共有側とクライアント側の両方にある場合（`config`）は、
namespace を path で重複排除します。

### 3. ホスト構造体に実装する

```rust
pub struct ClientHost<'a, 'b, 'c> {
    pub call: &'a mut Call<'b, 'c>,
}

// 各名前空間の trait を実装する。実装ファイルは api/host/<namespace>.rs。
impl WorkspaceApi for ClientHost<'_, '_, '_> { ... }
impl FsApi for ClientHost<'_, '_, '_> { ... }
impl ConfigReadApi for ClientHost<'_, '_, '_> { ... }
impl ConfigApi for ClientHost<'_, '_, '_> { ... }
// ... context, document, log, process, editor, keymap, ui.*, commands, views,
// notifications も同じ形で実装する。

impl vy_host_api::HostRoot for ClientHost<'_, '_, '_> {}
impl vy_host_api::HostApi for ClientHost<'_, '_, '_> {}
impl ClientHostApi for ClientHost<'_, '_, '_> {}
```

`vy-app` の dispatch は `ClientHostApi::dispatch` だけを呼びます。宣言ベースの
registry はもうありません。

### 4. イベントは trait の関連定数として宣言する

イベントはハンドラではないので、trait 内の `#[vy_api_event]` 付き関連定数として
宣言します。

```rust
#[vy_api_trait("workspace")]
pub trait WorkspaceApi {
    // methods ...

    /// Fires when the workspace folders change.
    #[vy_api_event(
        "workspace.onDidChangeWorkspaceFolders",
        payload = WorkspaceFoldersChange
    )]
    const ON_DID_CHANGE_WORKSPACE_FOLDERS: ();
}
```

`#[vy_api_trait]` は関連定数を trait から取り除き、`EventDecl` と `EVENTS` を
生成します。`host_api!` は `module::EVENTS` を集めるので、facade ごとの
`events_fn` が共有イベントとクライアントイベントをまとめます。

すべての名前空間を trait 化したため、イベントもすべて trait の関連定数として
宣言されています。`event_decl!` は、今後 trait ではない場所でイベントを宣言する
場合のために残しています。

## 検証した範囲

すべての名前空間を trait 化しました。

- 共有: `config`（読み取り）、`context`、`document`、`fs`、`log`、`process`、
  `workspace`
- クライアント: `commands`、`config`（書き込み）、`editor`、`keymap`、
  `notifications`、`ui`、`ui.gutter`、`ui.hover`、`ui.picker`、`ui.statusline`、
  `views`

主な確認項目:

- `fs` / `process` の `Reply::Deferred` と `ServerRpc`
- `views` / `editor` の `HostAction`
- `config` の共有 trait とクライアント trait の分割
- `commands` の `manual` と `typed_args`
- `host_api!` による facade、dispatch、`METHODS` / `NAMESPACE` / `EVENTS` の収集
- `vy-server` の `CoreHost` による共有 trait の実装
- `fs` のケイパビリティ許可と拒否
- 生成される TypeScript が変わらないこと

### 検証で見つかった点

`fs` のケイパビリティは、宣言では `${workspace}/**` のようなパスを使いますが、
API は `file:` URI を渡します。そのままでは glob が一致しないため、クライアントと
`CoreHost` の両方で `file:` URI をパスに変換してから照合するようにしました。

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
- `fs` / `process` / `editor` などで先に検証してから全体へ広げる

## 見直しの条件

1. facade trait の dispatch が大きくなりすぎる。
2. ホスト構造体のライフタイムや可変借用が扱いにくくなる。
3. trait の実装漏れよりも、宣言ベースの柔軟さが欲しくなる。

## 関連する未決定事項

- `CoreHost` を実際のプラグインホストへ接続する（Phase 6）。
- `&mut dyn HostApi` を使うか、generic のままにするか。
- `backend.d.ts` と `vy-api-codegen` の分離。
