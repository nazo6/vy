# vy-api

Type definitions for the module `init.ts` imports:

```typescript
import { commands, config, editor, keymap } from "vy-api";
```

The API is available globally as `vy` at runtime, or can be imported from
`vy-api`:

```typescript
vy.config.set({ "ui.fontSize": 14 });
```

The module itself is provided by the QuickJS host at runtime; it does not exist
as a file. Point the TypeScript compiler at this package when you write an
`init.ts`:

```json
{
    "compilerOptions": {
        "moduleDetection": "force",
        "paths": { "vy-api": ["./packages/vy-api/index.d.ts"] }
    },
    "include": ["init.ts", "./packages/vy-api/index.d.ts"]
}
```

`index.d.ts` is generated from the `#[vy_api_namespace]` declarations in
`crates/vy-app`; do not edit it. The declarations that need JavaScript state in
the shim (command handlers, the edit builder, `Document`, slot handles, pickers,
views, and the widget builders) live in `overrides.d.ts` and are composed into
the generated file. Types shared with the Rust protocol live in
[`packages/vy-api-types`](../vy-api-types/README.md). Both are refreshed by
`cargo test --workspace export_bindings`.

## API reference

The declarations are rendered as a searchable HTML reference with TypeDoc:

```sh
pnpm install --frozen-lockfile
cargo test --workspace export_bindings
pnpm docs:api
```

Open `target/docs/vy-api/index.html`. `pnpm docs:api:serve` builds the site and
serves it at <http://localhost:8000>. The output is generated into `target/` and
is not committed. `docs.d.ts` is the TypeDoc entry point; it only re-exports the
hand-written sugar interfaces so their methods appear in the reference.

To type the arguments of your own commands, augment `CommandMap`:

```typescript
declare module "vy-api" {
    interface CommandMap {
        "user.greet": { name: string };
    }
}
```

`commands.execute("user.greet", ...)` then completes the argument object. The
general overload still accepts other ids, so wrong arguments are not a type
error; runtime validation is planned for a later phase.
