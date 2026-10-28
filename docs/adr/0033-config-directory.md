---
id: "0033"
title: "設定ディレクトリはすべての OS で ~/.config/vy に統一する"
type: "adr"
status: "accepted"
category: "architecture"
date: 2026-10-28
tags: ["layout", "filesystem", "config-paths", "macos"]
supersedes: []
superseded_by: null
related_adrs: ["0010", "0020"]
---

# ADR 0033: 設定ディレクトリはすべての OS で `~/.config/vy` に統一する

- 状態: 採用
- 日付: 2026-10-28
- カテゴリー: architecture
- 実装: `crates/vy-app/src/paths.rs` の `config_base()`。仕様は
  [`docs/specs/layout.md`](../specs/layout.md) に反映済み

## 文脈

`crates/vy-app/src/paths.rs` は `dirs` クレートに場所を任せていました。macOS では
`dirs::config_dir()` が `~/Library/Application Support` を返すため、設定は
`~/Library/Application Support/vy` に置かれます。一方 `docs/specs/layout.md` 以外の
ドキュメント（`README.md`、`docs/specs/` の各ファイル、`examples/init.ts`）は
一貫して `~/.config/vy` と書いていました。

この不整合は実際に混乱を生みました。利用者が `~/.config/vy/init.ts` を置いても、
アプリは `~/Library/Application Support/vy/init.ts` を探すため
「no init.ts」と表示されます。

さらに macOS では `dirs::config_dir()` と `dirs::data_dir()` が同じ場所を返します。
その結果 `Paths::config` と `Paths::data` が同一ディレクトリを指し、利用者が編集する
`init.ts` / `settings.json` と、アプリ内部の `workspace-state/` が同じ階層に混在して
いました。

## 決定ドライバ

1. **ドキュメントとの一致**: `~/.config/vy` と書かれている既存の説明を正しくする。
2. **ターミナルエディタの慣習**: Vim、Neovim、Helix などと同じ場所に設定を置く。
3. **設定とデータの分離**: 利用者が編集するファイルと、アプリ内部の状態を混ぜない。
4. **macOS の作法との両立**: `~/Library` の使い分けを壊さない。
5. **移行コスト**: 既存の `VY_CONFIG_HOME` / `VY_DATA_HOME` による上書きを維持する。

## 検討した選択肢

| 選択肢                                            | 内容                                                                         | 評価                                                                            |
| :------------------------------------------------ | :--------------------------------------------------------------------------- | :------------------------------------------------------------------------------ |
| A. `dirs` のまま、ドキュメントを macOS 向けに直す | `~/Library/Application Support/vy` を正とし、README を修正する               | 変更は小さいが、ターミナルエディタの慣習とずれ、config と data の重複も残る     |
| B. すべての OS で `~/.config/vy`（採用）          | macOS でも `~/.config/vy` を使い、data と cache はプラットフォーム既定のまま | ドキュメントと一致し、慣習にも合う。data 側が別ディレクトリになり重複も解消する |
| C. `dirs` をやめて全 OS で独自規則                | Linux も含めて `~/.vy` などに統一する                                        | XDG に従う Linux 利用者の期待を裏切り、`XDG_CONFIG_HOME` も無視することになる   |

## 決定

設定ディレクトリはすべての Unix 系 OS で `~/.config/vy`、Windows で
`%APPDATA%\vy` とします。

- `VY_CONFIG_HOME` が設定されていればそれを使います。
- Unix 系では `XDG_CONFIG_HOME` も尊重します（未設定なら `~/.config`）。
- data と cache はこれまでどおり `dirs::data_dir()` / `dirs::cache_dir()` の
  プラットフォーム既定を使います。macOS では `~/Library/Application Support/vy` と
  `~/Library/Caches/vy` になり、設定とは別の場所です。

実装は `crates/vy-app/src/paths.rs` の `config_base()` に閉じ込め、`Paths` の
構造は変えません。

## 帰結

### 良い面

- `README.md`、`docs/specs/`、`examples/init.ts` の記述と実際のパスが一致する。
- 設定はホーム直下の `~/.config/vy` にあり、dotfiles 管理やターミナルからの編集が
  しやすい。
- macOS で `config` と `data` が分かれ、`workspace-state/` が設定ファイルの隣に
  現れなくなる。

### 悪い面

- macOS の File System Programming Guide がユーザー向けファイルに推奨する
  `~/Library/Application Support` からは外れる。GUI アプリとして配布する段階で
  再検討の余地がある。
- macOS で既に `~/Library/Application Support/vy` に設定を置いていた利用者は、
  `~/.config/vy` へ移動するか `VY_CONFIG_HOME` を設定する必要がある。移行処理は
  入れない（利用者が 1 人の開発段階のため）。

### 見直す条件

- macOS の GUI アプリとして配布し、設定を Finder から扱わせたくなったとき。
- Apple のガイドラインに沿うことが配布審査や利用者の期待で重要になったとき。

その場合は本 ADR を置き換える新しい ADR を追加します。
