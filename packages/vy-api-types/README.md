# vy-api-types

Types generated from the Rust crates with `ts-rs`. Do not edit the files in this
directory by hand.

- `vy-core` exports `Position`, `Range`, `Selection`, `TextEdit`, and `Uri`.
- `vy-proto` exports `ErrorCode`.

Regenerate after changing a type:

```sh
cargo test -p vy-core -p vy-proto export_bindings
```

The output directory comes from `TS_RS_EXPORT_DIR` in `.cargo/config.toml`.
`cargo test --workspace` runs the same export tests, so a stale directory shows
up as a diff in CI.
