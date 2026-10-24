# vy-api-types

Types generated from the Rust API declarations. Do not edit the files in this
directory by hand.

The declarations live in `vy-host-api` and `vy-client-api`. The generator walks
the `TypeCollector` of every method and event, so each file comes from the same
source as the TypeScript signatures.

Regenerate after changing a declaration or type:

```sh
cargo run -p vy-api-codegen -- client
```

To compare without writing files:

```sh
cargo run -p vy-api-codegen -- client --check
```

The output directory comes from `TS_RS_EXPORT_DIR` in `.cargo/config.toml`.
`CommandId.ts` and `SettingsKey.ts` come from the client metadata in
`vy-client-api`; the other files come from the API declarations.
