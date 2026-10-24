import { commands, keymap } from "vy-api";

declare module "vy-api" {
    interface CommandMap {
        "test.add": { a: number; b: number };
        "test.ping": undefined;
    }
}

commands.registerCommand("test.add", (args) => args.a + args.b);
commands.execute({ id: "test.add", args: { a: 1, b: 2 } });
commands.execute({ id: "vy.history.undo" });
commands.execute({ id: "test.ping" });
keymap.bind({ keys: "x", command: "test.add" });
keymap.bind({ keys: "x", command: "vy.history.undo" });

// @ts-expect-error a known command requires all of its arguments
commands.execute({ id: "test.add", args: { a: 1 } });

// @ts-expect-error a known command handler receives the declared arguments
commands.registerCommand("test.add", (args: string) => args);

// @ts-expect-error built-in command ids cannot be registered
commands.registerCommand("vy.history.undo", () => {});
