# vy-api

Type definitions for the module `init.ts` imports:

```typescript
import { commands, config, editor, keymap } from "vy-api";
```

The API is available globally as `vy` at runtime, or can be imported from
`vy-api`:

```typescript
vy.config.set({ values: { "ui.fontSize": 14 } });
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

`index.d.ts` is generated from the `#[vy_api_trait]` declarations in
`crates/vy-host-api` and `crates/vy-client-api`; do not edit it. The per-type
declarations and the `HostMethods`, `HostEvents`, `CommandId`, and `SettingsKey`
helpers live in [`types/index.ts`](./types/index.ts). The only hand-written
declaration is the command callback wrapper in `overrides.d.ts`.

Regenerate the declarations after changing an API type:

```sh
cargo run -p vy-api-codegen -- client
```

To verify without writing files:

```sh
cargo run -p vy-api-codegen -- client --check
```

## API reference

The declarations are rendered as a searchable HTML reference with TypeDoc:

```sh
pnpm install --frozen-lockfile
cargo run -p vy-api-codegen -- client
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
