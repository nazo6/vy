# vy-api

Type definitions for the module `init.ts` imports:

```typescript
import { commands, config, editor, keymap } from "vy-api";
```

The module itself is provided by the QuickJS host at runtime; it does not exist
as a file. Point the TypeScript compiler at this package when you write an
`init.ts`:

```json
{
    "compilerOptions": {
        "paths": { "vy-api": ["./packages/vy-api/index.d.ts"] }
    }
}
```

`index.d.ts` is generated from the method table in `crates/vy-api-schema`; do
not edit it. The declarations that need JavaScript state in the shim (command
handlers, the edit builder, `Document`, slot handles, pickers, views, and the
widget builders) live in `overrides.d.ts` and are composed into the generated
file. Types shared with the Rust protocol live in
[`packages/vy-api-types`](../vy-api-types). Both are refreshed by
`cargo test --workspace export_bindings`.

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
