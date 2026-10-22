# vy-api

Type definitions for the module `init.ts` imports:

```typescript
import { commands, config, editor, keymap } from "vy-api";
```

The module itself is provided by the QuickJS host at runtime; it does not exist
as a file. `index.d.ts` describes the Phase 2 surface documented in
[`docs/api.md`](../../docs/api.md). Point the TypeScript compiler at this
package when you write an `init.ts`:

```json
{
    "compilerOptions": {
        "paths": { "vy-api": ["./packages/vy-api/index.d.ts"] }
    }
}
```

Types shared with the Rust protocol live in
[`packages/vy-api-types`](../vy-api-types), which `cargo test export_bindings`
regenerates.
