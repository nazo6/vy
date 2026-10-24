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

`api-base.d.ts` and [`types/index.ts`](./types/index.ts) are generated from the
`#[vy_api_trait]` declarations in `crates/vy-host-api` and `crates/vy-client-api`.
The per-type declarations and the `HostMethods`, `HostEvents`,
`BuiltinCommandMap`, and `SettingsKey` helpers live in `types/`.

The runtime implementation and its JS-facing types live in
`crates/vy-js/src/shim.ts`. TypeScript emits `shim.d.ts` from it, and
`index.d.ts` composes `types/`, `api-base.d.ts`, and `shim.d.ts`. Do not edit the
generated files by hand.

Regenerate the declarations and the shim after changing an API type:

```sh
pnpm generate:api
```

To verify the Rust-generated part without writing files:

```sh
cargo run -p vy-api-codegen -- client --check
```

## API reference

The declarations are rendered as a searchable HTML reference with TypeDoc:

```sh
pnpm install --frozen-lockfile
pnpm generate:api
pnpm docs:api
```

Open `target/docs/vy-api/index.html`. `pnpm docs:api:serve` builds the site and
serves it at <http://localhost:8000>. The output is generated into `target/` and
is not committed. `docs.d.ts` is the TypeDoc entry point.

To type the arguments of your own commands, augment `CommandMap`:

```typescript
declare module "vy-api" {
    interface CommandMap {
        "user.greet": { name: string };
    }
}
```

`commands.registerCommand("user.greet", (args) => ...)` and
`commands.execute({ id: "user.greet", args: { name: "Ada" } })` then complete and
check the argument object. Commands with no arguments can use `undefined` as the
map value.

Built-in command ids are generated separately in `BuiltinCommandMap`. They are
valid for `commands.execute` and `keymap.bind`, but not for
`commands.registerCommand`; the host reserves ids starting with `vy.`.
