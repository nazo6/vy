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

### 変更時のドキュメント同期（必須）

コード・仕様・設計を変更したら、同じ変更の中で関連するドキュメントも更新します。ドキュメントの更新を次の作業や別 PR に先送りしません。

1. **実装を変えたとき**: 利用者に見える振る舞い、API、設定キー、プロトコル、コマンドが変わったら、[`docs/specs/`](./docs/specs/README.md) の該当ファイルを現在の実装に合わせます。どの仕様書か分からないときは `rg` でコマンド名・API 名・設定キーを検索し、言及箇所をすべて確認します。
2. **決定を変えたとき**: 既存 ADR は書き換えず、新しい ADR を追加します。置き換えた ADR は `superseded/` へ移します。
3. **ADR の実装が完了したとき**: `status` を `implemented` にし、`docs/specs/` に反映済みであることを確認します。未完了なら `accepted` のまま、進捗と残作業を本文に書きます。
4. **ADR を追加・更新したとき**: [`docs/adr/README.md`](./docs/adr/README.md) の索引のステータスを揃え、`related_adrs` と `superseded_by` を更新します。
5. **まとまった作業をしたとき**: [`docs/worklogs/`](./docs/worklogs/README.md) に日付・関連 ADR・検証結果・残課題を記録します。
6. **索引と例**: `README.md`、[`docs/README.md`](./docs/README.md)、各ディレクトリの `README.md`、`examples/` のコードが、変更後の内容と食い違っていないか確認します。

frontmatter を持つ文書を変更したら `updated_at` を更新します。変更後は `git diff` を見て、コードだけ・ドキュメントだけの差分が残っていないか確認します（意図した例外は除く）。

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

ドキュメントだけを変更した場合も `pnpm format:check` を通し、リンク切れと
索引（`docs/README.md`、`docs/specs/README.md`、`docs/adr/README.md`、
`docs/worklogs/README.md`）の整合を確認します。
