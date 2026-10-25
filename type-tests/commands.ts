import type {} from "vy-api";

declare global {
    namespace Vy {
        interface CommandMap {
            "test.add": { a: number; b: number };
            "test.ping": undefined;
        }
    }
}

vy.commands.registerCommand("test.add", (args) => args.a + args.b);
vy.commands.execute({ id: "test.add", args: { a: 1, b: 2 } });
vy.commands.execute({ id: "vy.history.undo" });
vy.commands.execute({ id: "test.ping" });
vy.keymap.bind({ keys: "x", command: "test.add" });
vy.keymap.bind({ keys: "x", command: "vy.history.undo" });

// @ts-expect-error a known command requires all of its arguments
vy.commands.execute({ id: "test.add", args: { a: 1 } });

// @ts-expect-error a known command handler receives the declared arguments
vy.commands.registerCommand("test.add", (args: string) => args);

// @ts-expect-error built-in command ids cannot be registered
vy.commands.registerCommand("vy.history.undo", () => {});

vy.commands.getCommands().then((commands) => {
    for (const command of commands) {
        command.id satisfies string;
        command.category satisfies string;
        command.exNames satisfies Array<string>;
        command.args satisfies Array<{ name: string; kind: string }>;
    }
});
