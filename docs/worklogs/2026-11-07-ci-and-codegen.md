---
title: "GitHub Actions CI ワークフローと codegen 差分検証の整備 (ADR 0031 Phase 3)"
type: "worklog"
date: 2026-11-07
phase: "Phase 3"
tags: ["ci", "codegen", "infrastructure"]
---

# [Worklog] GitHub Actions CI ワークフローと codegen 差分検証の整備 (ADR 0031 Phase 3)

- **日付**: 2026-11-07
- **関連フェーズ/タスク**: ADR 0031 Phase 3「開発基盤（CI と codegen）」
- **目的**: CI ワークフローと型生成・差分検証の自動テスト基盤を整備し、生成忘れやフォーマット・型チェック違反を自動検出できるようにする。

## 作業内容

1. **Codegen 検証範囲の拡張と統合テスト化**:
   - `crates/vy-api-codegen` をライブラリクレート（`src/lib.rs`）とバイナリクレート（`src/main.rs`）の両立構成に整理。
   - `check_client` に `check_shims` を追加し、`packages/vy-api/shim.d.ts` および `crates/vy-js/src/generated/shim.js` の存在と `target/api-shim` との整合性を検証対象に追加。
   - `crates/vy-api-codegen/tests/codegen.rs` を新設し、通常の `cargo test` 実行時に生成物の差分検証（`check_client`）が自動実行されるようにした。

2. **GitHub Actions CI ワークフローの追加**:
   - `.github/workflows/ci.yml` を新設。
   - `cargo fmt --all --check`、`pnpm format:check`、`pnpm check:types`、`cargo clippy --workspace --all-targets`、`cargo run -p vy-api-codegen -- --check`、`pnpm generate:api` 後の clean diff 検証、および `cargo test --workspace` を自動実行。
   - Linux ランナー環境に必要な GPUI およびフォント描画依存ライブラリ（`pkg-config`, `libxkbcommon-dev`, `libxkbcommon-x11-dev`, `libwayland-dev`, `libfontconfig1-dev`）のインストール手順を明記。
   - `docs/development.md` の検証セクションを実態に合わせて更新。

## 検証結果・測定値

- `cargo test -p vy-api-codegen`: `test client_declarations_are_up_to_date ... ok` 通過。
- `cargo run -p vy-api-codegen -- --check`: 差分 0 件で合格。
- `pnpm generate:api && git diff --exit-code packages crates/vy-js/src/generated`: 生成物クリーン確認。
