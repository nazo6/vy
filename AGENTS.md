# AGENTS.md

Vy の開発方針です。作業を始める前に [`docs/README.md`](./docs/README.md) を確認し、関連する仕様書や設計ドキュメントを読んでください。

## 言語

- ドキュメント（`README.md`、`docs/`）は日本語で書きます。
- コード中のコメント、doc
  コメント、ログ、エラーメッセージ、利用者に見せる文字列は
  英語で書きます。将来の多言語化に備え、コードに日本語の文言を埋め込みません。
- テストデータとして日本語や絵文字を使うことはあります（UTF-16 の検証など）。
- コミットメッセージは英語にします。

## ドキュメントと設計

設計の決定や現在の仕様については `docs/` 配下を参照してください。個別のアーキテクチャや設計ルールはコードに直接埋め込まず、ドキュメントを正とします。

- **現在の仕様・設計**: [`docs/specs/`](./docs/specs/README.md)
- **意思決定・提案の記録 (ADR)**: [`docs/adr/`](./docs/adr/README.md)
  - 設計上の決定を変えるときは既存の ADR を書き換えず、新しい ADR を追加します。未決定の提案やロードマップも ADR として起草・検討します。
- **作業ログ**: [`docs/worklogs/`](./docs/worklogs/README.md)
- **開発方針・運用ルール**: [`docs/development.md`](./docs/development.md)

## 検証

変更したら、次の 4 つを通します。

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --all --check
pnpm format:check
```

コードを変更したら、`rg -n '[ぁ-んァ-ヶ一-龠]' crates` で日本語のコメントや
メッセージが残っていないか確認します（テストデータは除きます）。

Rust 以外のファイルは Prettier（TypeScript / JavaScript / JSON / YAML / Markdown）と
tombi（TOML）で整形します。保存時の自動整形が有効ですが、手で直したときは
`pnpm format` を実行し、`pnpm format:check` で確認します
（[`docs/development.md`](./docs/development.md) の §8）。
