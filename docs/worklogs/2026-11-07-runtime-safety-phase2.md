---
title: "サンドボックス境界の強制 (ADR 0031 Phase 2)"
type: "worklog"
date: 2026-11-07
phase: "Phase 2"
tags: ["security", "sandbox", "capabilities"]
---

# [Worklog] サンドボックス境界の強制 (ADR 0031 Phase 2)

- **日付**: 2026-11-07
- **関連フェーズ/タスク**: ADR 0031 Phase 2「サンドボックスの境界」
- **目的**: プラグインが `${workspace}/**` 権限から `..` やシンボリックリンクを経由してワークスペース外へ脱出するのを完全に防止する。

## 作業内容

1. **パス正規化とケイパビリティ照合の強化**:
   - `crates/vy-capabilities/src/lib.rs` に字句的パス正規化 `normalize_path(&Path) -> PathBuf` および `normalize_path_str(&str) -> String` を新設。ファイルシステムにアクセスすることなく `.` と `..` のセグメントを畳み込む。
   - `Capability::covers` で、`fs:` 系のケイパビリティ照合時にパターンと要求対象パスの双方を正規化してから glob 照合を実施。
   - `crates/vy-app/src/api/mod.rs` の `check_capability` で、URI から抽出したパスを正規化し、さらに実体パスが存在する場合は `canonicalize` 後のパスについてもケイパビリティ照合を二重に検証。

2. **サーバー側でのサンドボックス強制**:
   - `crates/vy-server/src/session/fs.rs` に `canonicalize_with_ancestors` および `Session::resolve_fs_path` を実装。
   - `session.workspace` が設定されている場合、要求されたファイルパス（新規作成・削除・存在確認・ディレクトリ作成・移動・複製・読み書きすべて）の実体パスがワークスペース配下に収まっているかを検証。ワークスペース外への脱出は `ErrorCode::CapabilityDenied` で確実に拒否。
   - `crates/vy-server/src/session/host.rs` の `CoreHost` でも同様にパス正規化とワークスペース境界検査を実施。

3. **セキュリティ結合テストの追加**:
   - `crates/vy-server/src/session/tests.rs` に `test_sandbox_enforcement_and_symlinks` を追加。
   - ワークスペース外への `..` 脱出試行、パーセントエンコード経由の脱出試行、ワークスペース外を指すシンボリックリンクの読み書き・削除・移動がすべて `ErrorCode::CapabilityDenied` で遮断されることを検証。

## 発生した課題と対処

- **`fs_exists` でのエラー握りつぶし**:
  - `fs_exists` が `resolve_fs_path` の失敗時に `exists: false` を返していたため、ワークスペース外のファイルに対する存在確認で `CapabilityDenied` ではなく成功（`exists: false`）になっていた。セキュリティ上、境界外へのプローブも `CapabilityDenied` エラーとして返すよう修正。

## 検証結果・測定値

- `vy-capabilities`: 全 7 単体テスト通過（字句的正規化、`..` 畳み込み、境界外脱出拒否を含む）。
- `vy-server`: 全 33 単体テスト + 2 結合テスト通過（`test_sandbox_enforcement_and_symlinks` を含む）。
- `vy-app`: 全 282 単体テスト通過。
- `cargo clippy`: 警告 0 件。
