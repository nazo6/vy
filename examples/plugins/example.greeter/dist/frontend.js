import { commands, notifications, ui } from "vy-api";

// 1. Notify that the plugin is activated
notifications.show("Greeter Plugin activated!");

// 2. Add an item to the statusline
ui.statusline.createItem({
  id: "greeter.status",
  text: " [Greeter: Ready]",
  alignment: "right",
  priority: 10,
});

// 3. Register a command
commands.registerCommand("example.greeter.hello", async () => {
  await notifications.show("Hello! Greeter Plugin is working properly.");
});
