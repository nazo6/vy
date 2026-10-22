// Example ~/.config/vy/init.ts for Phase 2.
//
// Copy it to `~/.config/vy/init.ts`. Only the APIs in `packages/vy-api` exist:
// commands, keymap, config, editor, workspace, notifications, log, remote, and
// context. External plugins and UI views arrive in Phase 3.

import { commands, config, editor, notifications } from "vy-api";

config.set({
  "ui.fontFamily": "JetBrains Mono",
  "ui.fontSize": 14,
  "ui.lineNumbers": "relative",
  "editor.transactionIdleMs": 500,
});

commands.registerCommand("user.uppercase-document", async () => {
  const document = await editor.activeDocument();
  if (!document) {
    return;
  }

  const text = await document.getText();
  await editor.edit((builder) => {
    builder.replace(document.fullRange(), text.toUpperCase());
  });

  await notifications.show("Uppercased the document");
});
