// Hand-written declarations for `vy-api`.
//
// Everything else in `index.d.ts` is generated from the
// `#[vy_api_namespace]` declarations in `crates/vy-app`. This file only holds
// the command callback wrapper, which needs a JavaScript function.

import type { CommandId } from "../vy-api-types";
import type { Disposable } from "./index";

/** Registers command handlers and runs commands by id. */
export interface CommandsSugar {
  /**
   * Registers a handler for a command id.
   *
   * @example
   * ```ts
   * const registration = commands.registerCommand("my.greet", () => {
   *   notifications.show({ message: "Hello" });
   * });
   * ```
   */
  registerCommand(
    id: CommandId | (string & {}),
    handler: (args?: unknown) => unknown,
  ): Disposable;
}
