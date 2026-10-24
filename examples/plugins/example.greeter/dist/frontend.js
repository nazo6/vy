import { commands, notifications, ui } from "vy-api";

// 1. Notify that the plugin is activated
await notifications.show({ message: "Greeter Plugin activated!" });

// 2. Add an item to the statusline
await ui.statusline.createItem({
  id: "greeter.status",
  text: " [Greeter: Ready]",
  alignment: "right",
  priority: 10,
});

// 3. Register a command
commands.registerCommand("example.greeter.hello", async () => {
  await notifications.show({ message: "Hello! Greeter Plugin is working properly." });
});
