# ペインとレイアウト

画面の構成に関する決定をこのファイルにまとめます。モデル、操作の意味論、API、メッセージ、保存の単位をここに置き、他のドキュメントからはこのファイルを参照します。

ファイルとフォルダの配置は [`layout.md`](./layout.md)、API 全体の一覧は
[`api.md`](./api.md)、メッセージの共通部分は [`protocol.md`](./protocol.md)
にあります。

## 1. 方針

- 固定のサイドバー、パネル、ボトムパネルを作りません。画面に出るものはすべてペインです。
- ペインは、ウィンドウ（タブ）の中のレイアウト木の葉か、フローティングのどちらかです。Vim
  のウィンドウ分割に近いモデルにします。
- レイアウトはコアが持ちます。クライアントは操作の意図を送り、確定した木を描画します。
- サイドバーに相当するものは、左に寄せたペインの並びです。専用の領域は設けません。

この形にする理由は3つあります。

1. 固定の領域を増やすと、プラグインが使える場所と、保存するレイアウトの形式が領域ごとに増えます。
2. すべてをペインにすると、開く、分ける、閉じる、移動するの4つの操作で扱えます。
3. コアがレイアウトを持つと、リモート接続でもセッションの復元でも同じ形になります。

## 2. 用語

| 用語         | 意味                                                                        |
| :----------- | :-------------------------------------------------------------------------- |
| Window       | タブ1つ分。レイアウト木を1つ持つ                                            |
| Pane         | 画面の区画。レイアウト木の葉か、フローティング                              |
| View         | ペインの中身。editor、ui、list、text、image、webview、terminal のいずれか   |
| Split        | 2つの子を持つ分割                                                           |
| FloatingPane | 木に入らないペイン。矩形と重なり順を持つ                                    |
| Overlay      | 補完、ホバー、メニュー、コマンドパレット、Ex コマンドライン。ペインではない |

ステータスラインはペインではありません。エディタのビューが持つ行で、項目だけを差し替えます。

補完、ホバー、メニュー、コマンドパレット、Ex
コマンドラインは一時的なオーバーレイです。利用者が開いたり閉じたりする対象ではないため、ペインにせず、保存や復元の対象にもしません。Ex
の設計は [`ex-commands.md`](./ex-commands.md) にあります。

## 3. データモデル

```text
Window
  id: WindowId
  layout: LayoutNode
  floating: [FloatingPane]
  active_pane: PaneId
  revision: u64

LayoutNode
  Split { direction: horizontal | vertical, ratio: 0..1, first: LayoutNode, second: LayoutNode }
  Leaf  { pane: PaneId }

Pane
  id: PaneId
  view: ViewHandle
  title?: string

FloatingPane
  pane: PaneId
  rect { x, y, width, height }   // 0..1 の相対値
  z: u32
```

不変条件は次の5つです。

1. `Leaf` の `pane` は木の中で重複しない。
2. `Split` の `ratio` は 0 より大きく 1 より小さい。
3. ウィンドウには少なくとも1つの `Leaf` がある。
4. `active_pane` は、木かフローティングに実在するペインを指す。
5. `direction` は `horizontal` が左右の並び、`vertical`
   が上下の並びを表す。境界線の向きではなく、子が並ぶ向きで呼ぶ。

分割は2分木で表します。3つ以上に分けるときは、木を深くします。比率は画面の大きさが変わっても保ちます。

## 4. ビュー

| kind     | 内容                                             | 状態を持つ場所                                                       |
| :------- | :----------------------------------------------- | :------------------------------------------------------------------- |
| editor   | テキスト編集。バッファを表示する                 | バッファはコア、カーソルと選択と折りたたみとスクロールはクライアント |
| ui       | プラグインがウィジェット木で構成するネイティブUI | 木とフォーカスはクライアント、データは提供元                         |
| list     | 項目の一覧。ファイル一覧、検索結果               | 項目はクライアント、検索結果の元はコア                               |
| text     | 読み取り専用のテキスト。ログ、差分の要約         | クライアント                                                         |
| image    | 画像                                             | クライアント                                                         |
| webview  | Webview。Notebook、Markdown プレビュー           | クライアント                                                         |
| terminal | 端末                                             | 端末のプロセスはコア、表示はクライアント                             |

- 同じ文書を複数のペインで開けます。バッファはコアで共有し、カーソル、選択、スクロールはペインごとにクライアントが持ちます（[`architecture.md`](./architecture.md)
  の「状態の所在」）。
- ビューの種類はコアが提供するものに限ります。プラグインはビューを登録し、`ui`
  ビューではウィジェット木を渡します。GPUI の要素を JS
  から作ることはできません。UI の作り方は [`ui.md`](./ui.md) にまとめました。
- `webview` は最終手段です。Web の文書を表示するときだけ使い、操作を伴う UI には
  `ui` を使います。キー入力とフォーカスを共有できないためです。
- `singleton: true`
  のビューは、同じ種類を2つ開こうとしたときに既存のペインへフォーカスします。ファイル一覧のように1つで足りるものに使います。

## 5. 操作の意味論

| 操作         | 意味                                                           |
| :----------- | :------------------------------------------------------------- |
| open         | 対象のペインの指定した方向に新しいペインを作り、ビューを載せる |
| split        | 空のペインを作る。中身は後から入れる                           |
| close        | ペインを閉じる。最後の1つのときは失敗する                      |
| focus        | フォーカスを移す。フローティングも対象になる                   |
| move         | ペインを別の位置へ移す。木から外して入れ直す                   |
| resize       | `Split` の比率を変える                                         |
| openFloating | 木に入らないペインを作る                                       |

- 開く位置は、対象のペインに対する `left`、`right`、`above`、`below`
  で指定します。画面全体の固定位置は指定できません。
- 対象を省略したときは、フォーカス中のペインを基準にします。
- ペインを閉じたとき、空になった `Split` は畳みます。兄弟が1つ上に上がります。
- 最後のペインを閉じる操作は失敗します。ウィンドウを閉じる操作と区別するためです。
- 比率が最小の大きさを下回るときは、比率を調整します。クライアントが画面の大きさを変えたときも同じです。

## 6. 状態の流れ

1. プラグインかキーバインドが `panes.open` などを呼びます。
2. クライアントが要求をコアへ送ります。
3. コアが木を更新し、`revision` を上げます。
4. コアが `layout/did_change` で木全体を送ります。
5. クライアントは、受け取った `revision` が手元より新しいときだけ適用します。

クライアントは木を先に描きません。レイアウトの変更は入力の遅延に直結しないため、確定を待つ方が単純です。

木全体を送るのは、差分だけを送るとクライアントの取りこぼしで木がずれるためです。ペインの数は多くても数十なので、全体を送る費用は小さく抑えられます。

## 7. API

ケイパビリティはすべて「なし」です。レイアウトの操作は承認を必要としません。実行場所は
`both`
とします。レイアウト木の状態はコアが持ち、クライアントは確定した木を描画するためです。

### 7.1 ウィンドウ

| API                       | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------ | :------- | :------------- | :----- |
| `windows.active()`        | both     | なし           | P0     |
| `windows.list()`          | both     | なし           | P1     |
| `windows.create(options)` | both     | なし           | P2     |
| `windows.close(windowId)` | both     | なし           | P2     |

ウィンドウはタブ1つ分で、レイアウト木の入れ物です。

### 7.2 ペイン

| API                                 | 実行場所 | ケイパビリティ | 優先度 |
| :---------------------------------- | :------- | :------------- | :----- |
| `panes.activePane()`                | both     | なし           | P0     |
| `panes.open(options)`               | both     | なし           | P1     |
| `panes.split(options)`              | both     | なし           | P1     |
| `panes.close(paneId)`               | both     | なし           | P1     |
| `panes.focus(paneId)`               | both     | なし           | P1     |
| `panes.list()`                      | both     | なし           | P1     |
| `panes.getLayout(windowId)`         | both     | なし           | P1     |
| `panes.onDidChangeLayout(listener)` | local    | なし           | P1     |
| `panes.move(paneId, options)`       | both     | なし           | P2     |
| `panes.resize(splitId, ratio)`      | both     | なし           | P2     |
| `panes.setLayout(windowId, layout)` | both     | なし           | P2     |
| `panes.setTitle(paneId, title)`     | both     | なし           | P2     |
| `panes.openFloating(options)`       | both     | なし           | P2     |
| `panes.closeFloating(paneId)`       | both     | なし           | P2     |

`panes.open` は `view` と `placement` を受け取ります。`placement` は
`{ target: PaneId, direction: "left" | "right" | "above" | "below", ratio?: number }`
です。`target` を省略したときは、フォーカス中のペインを基準にします。

`panes.split` は空のペインを分けるだけです。中身を後から入れるときに使います。

### 7.3 ビュー

| API                             | 実行場所 | ケイパビリティ | 優先度 |
| :------------------------------ | :------- | :------------- | :----- |
| `views.register(type, options)` | local    | なし           | P1     |
| `views.unregister(type)`        | local    | なし           | P1     |
| `views.get(type)`               | local    | なし           | P2     |

`views.register` の `options`
には、`kind`、`title`、`singleton`、`preferredSize`、データの提供関数を指定します。`ui`
ビューでは、これに `render`、`actions`、`restore` を加えます。`kind`
は上の表の7種類です。`ui` ビューの木、キー入力、復元の扱いは [`ui.md`](./ui.md)
にあります。

### 7.4 エディタとの関係

`editor.open` は、フォーカス中のペインで文書を開きます。別のペインで開くときは
`options.placement` を渡します。ペイン自体を作るときは `panes.open` を使います。

## 8. メッセージ

| メソッド            | 種別         | 方向                    |
| :------------------ | :----------- | :---------------------- |
| `layout/get`        | Request      | クライアント → サーバー |
| `layout/set`        | Request      | クライアント → サーバー |
| `layout/did_change` | Notification | サーバー → クライアント |
| `pane/open`         | Request      | クライアント → サーバー |
| `pane/close`        | Request      | クライアント → サーバー |
| `pane/focus`        | Request      | クライアント → サーバー |
| `pane/resize`       | Request      | クライアント → サーバー |
| `pane/did_change`   | Notification | サーバー → クライアント |

```text
Notification {
    method: "layout/did_change",
    params: {
        "window_id": 1,
        "revision": 12,
        "layout": {
            "type": "split",
            "direction": "horizontal",
            "ratio": 0.25,
            "first": { "type": "leaf", "pane_id": 3 },
            "second": {
                "type": "split",
                "direction": "vertical",
                "ratio": 0.7,
                "first": { "type": "leaf", "pane_id": 4 },
                "second": { "type": "leaf", "pane_id": 5 }
            }
        },
        "floating": [
            { "pane_id": 9, "rect": { "x": 0.3, "y": 0.2, "width": 0.4, "height": 0.3 }, "z": 10 }
        ],
        "active_pane": 4,
        "reason": "open"
    }
}
```

`reason` には `open`、`close`、`focus`、`resize`、`move`、`restore`
のいずれかが入ります。クライアントは理由を使わなくても適用できます。

`pane/open` は `view`、`placement`、`params` を受け取ります。`view`
はビューの種類の名前か、`webview` のハンドルです。`params` はプラグインが決めた
JSON で、`ui` ビューの初期状態に使います。`placement` の `target`
を省略したときは、`active_pane` を基準にします。

## 9. 保存と復元

- レイアウト木はコアが保存します。ワークスペースごとに、開いているウィンドウとペインの構成を1つの状態として保存します。
- クライアントは、ウィンドウの大きさと接続先を `session.json` に保存します。
- 復元では、先にレイアウト木を戻し、そのあとにビューを戻します。ビューを開けなかった場合は、そのペインを空のエディタにします。木は壊しません。
- `ui` ビューは、コアが保存した `params` と `state`
  をプラグインに渡し、`restore`
  から木を作り直します。木そのものは保存しません。スクロール位置とフォーカスの履歴はクライアントが
  `session.json` に保存します。
- `webview` のハンドルは保存できません。プラグインが `state`
  から作り直せるときだけ復元し、それ以外は空のエディタにします。
- レイアウト木の `revision` は復元のたびに初期化します。クライアントは接続時に
  `layout/get` で取り直します。

## 10. 既定のレイアウト

```typescript
// ~/.config/vy/init.ts
const root = await panes.activePane();

await panes.open({
  view: "list",
  placement: { target: root, direction: "left", ratio: 0.25 },
});

await panes.open({
  view: "terminal",
  placement: { target: root, direction: "below", ratio: 0.3 },
});
```

この例は、左にファイル一覧、下に端末を置きます。どちらも専用の領域ではなく、ペインです。

## 11. 未決定事項

- 比率をピクセルで持つか、比率で持つか。画面の大きさが変わったときの扱いが変わります。
- フローティングのペインをキーボードだけで移動、リサイズできるようにするか。
- 1つのウィンドウに置けるペインの上限。
- 同じビューを複数のペインに置けるようにするか、`singleton` を既定にするか。

## 12. 実装状況（Phase 3 の途中）

実装済み:

- レイアウト木（`vy-core` の `Layout`）。分割は `pane/open` の `placement`
  で行い、 比率は 0.5 で固定です。組み込みコマンド（`vy.pane.split_right` / `vy.pane.split_down`）
  および Ex コマンド（`:vsp` / `:sp`）から分割できます。
- `pane/open`、`pane/focus`、`pane/close`、`layout/get` と
  `layout/did_change`。フォーカスは `Ctrl-w h/j/k/l` と `Ctrl-w w` で移せます。
- 同じ URI を開くと、既存の文書を新しいペインで共有します。カーソル、選択、
  モード、スクロール、IME の前編集はペインごとに独立しています。
- 最後に選択した範囲は文書ごとに覚え、`g v` でどのペインからでも復元できます。
- ペインをすべて閉じた文書は破棄します。最後の 1 ペインは閉じられません。

未実装:

- フローティング、比率の変更（`pane/resize`）、`layout/set`、
  保存と復元。
- `views.register` と `editor` 以外のビュー。`pane/open` は `view: "editor"`
  だけを 受け付けます。
- レジスタと `.` の履歴は文書ごとです。ワークスペース全体で共有するのは次の段階
  です。
