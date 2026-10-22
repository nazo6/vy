# AGENTS.md

Vy の開発方針です。作業を始める前に
[`docs/development.md`](./docs/development.md) と
[`docs/roadmap.md`](./docs/roadmap.md) を読んでください。

## 言語

- ドキュメント（`README.md`、`docs/`）は日本語で書きます。
- コード中のコメント、doc
  コメント、ログ、エラーメッセージ、利用者に見せる文字列は
  英語で書きます。将来の多言語化に備え、コードに日本語の文言を埋め込みません。
- テストデータとして日本語や絵文字を使うことはあります（UTF-16 の検証など）。
- コミットメッセージは英語にします。

## 設計

- 決定は `docs/` に書き、決定を変えるときは ADR を追加します
  （[`docs/adr/README.md`](./docs/adr/README.md)）。
- 決まっていない項目は各ドキュメントの「未決定事項」に置き、決めたら「決定済み」へ
  移します。
- クライアントとコアは常に別プロセスにします（[ADR 0002](./docs/adr/0002-process-model.md)）。
  同一プロセスでコアを呼ぶ近道は作りません。
- `vy-app` だけが GPUI に依存します。`vy-core` は描画に依存させません。

## 検証

変更したら、次の 3 つを通します。

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
```

コードを変更したら、`rg -n '[ぁ-んァ-ヶ一-龠]' crates` で日本語のコメントや
メッセージが残っていないか確認します（テストデータは除きます）。
