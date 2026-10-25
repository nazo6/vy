import type {} from "vy-api";
import { COMMAND_METADATA } from "vy-api";

declare global {
    namespace Vy {
        interface CommandMap {
            "test.add": { a: number; b: number };
            "test.ping": undefined;
        }
    }
}

vy.commands.registerCommand("test.add", (args) => args.a + args.b);
vy.commands.register(
    {
        id: "test.greet",
        title: "Greet",
        category: "plugin",
        exNames: ["greet"],
        args: [{ name: "who", kind: "text", required: true }],
    },
    (args) => args,
);
vy.commands.register({ id: "test.other" }, () => {});
vy.commands.execute({ id: "test.add", args: { a: 1, b: 2 } });
vy.commands.execute({ id: "vy.history.undo" });
vy.commands.execute({ id: "test.ping" });
vy.keymap.bind({ keys: "x", command: "test.add" });
vy.keymap.bind({ keys: "x", command: "vy.history.undo" });
vy.keymap.bind({
    keys: "x",
    command: "vy.editor.open",
    args: { uri: "README.md" },
});

// @ts-expect-error a known command requires all of its arguments
vy.commands.execute({ id: "test.add", args: { a: 1 } });

// @ts-expect-error a known command handler receives the declared arguments
vy.commands.registerCommand("test.add", (args: string) => args);

// @ts-expect-error built-in command ids cannot be registered
vy.commands.registerCommand("vy.history.undo", () => {});

// @ts-expect-error the category must be one of the declared values
vy.commands.register({ id: "test.bad", category: "nope" }, () => {});

vy.commands.getCommands().then((commands) => {
    for (const command of commands) {
        command.id satisfies string;
        command.category satisfies string;
        command.exNames satisfies Array<string>;
        command.args satisfies Array<{ name: string; kind: string }>;
    }
});

for (const command of COMMAND_METADATA) {
    command.category satisfies string;
    command.exNames satisfies Array<string>;
    command.args satisfies Array<{ name: string; kind: string }>;
}
