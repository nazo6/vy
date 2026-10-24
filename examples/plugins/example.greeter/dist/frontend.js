// 1. Notify that the plugin is activated
await vy.notifications.show({ message: "Greeter Plugin activated!" });

// 2. Add an item to the statusline
await vy.ui.statusline.createItem({
  id: "greeter.status",
  text: " [Greeter: Ready]",
  alignment: "right",
  priority: 10,
});

// 3. Register a command
vy.commands.registerCommand("example.greeter.hello", async () => {
  await vy.notifications.show({ message: "Hello! Greeter Plugin is working properly." });
});
