import { greeting } from "./lib/greeting";

vy.commands.registerCommand("example.source.hello", async () => {
  await vy.notifications.show({ message: greeting("Vy") });
});

await vy.notifications.show({ message: "Source plugin loaded" });
