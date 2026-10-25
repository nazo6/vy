# vy-api

Type definitions for the global `vy` object used by `init.ts` and plugins.

```typescript
// ~/.config/vy/init.ts
import type {} from "vy-api";

vy.config.set({ values: { "ui.fontSize": 14 } });
```

`vy` is installed by the QuickJS host before a script runs. This package contains
only type declarations; the runtime API is the global `vy`. The type-only import
above makes `vy` and `Vy.CommandMap` visible to TypeScript and is erased at build
time.

## Setup

Install the package as a development dependency and include it once in the
program:

```typescript
// env.d.ts
import type {} from "vy-api";
```

In this repository, `tsconfig.json` maps `vy-api` to
[`index.d.ts`](./index.d.ts).

`api-base.d.ts` and [`types/index.ts`](./types/index.ts) are generated from the
`#[vy_api_trait]` declarations in `crates/vy-host-api` and `crates/vy-client-api`.
The Specta-generated `bindings.ts` and the `HostMethods`, `HostEvents`,
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

To type the arguments of your own commands, augment `Vy.CommandMap`:

```typescript
declare global {
    namespace Vy {
        interface CommandMap {
            "user.greet": { name: string };
        }
    }
}
export {};
```

`vy.commands.registerCommand("user.greet", (args) => ...)` and
`vy.commands.execute({ id: "user.greet", args: { name: "Ada" } })` then complete
and check the argument object. Commands with no arguments can use `undefined` as
the map value. Add `export {}` when the file has no other import or export, so
that `declare global` is allowed.

Built-in command ids are generated separately in `BuiltinCommandMap`. They are
valid for `vy.commands.execute` and `vy.keymap.bind`, but not for
`vy.commands.registerCommand`; the host reserves ids starting with `vy.`.
