// Example ~/.config/vy/init.ts for Phase 2.
//
// Copy it to `~/.config/vy/init.ts`. Only the APIs in `packages/vy-api` exist:
// commands, keymap, config, editor, document, workspace, context,
// notifications, log, fs, process, ui, and views.

import type {} from "vy-api";

vy.config.set({
  values: {
    "ui.fontFamily": "JetBrains Mono",
    "ui.fontSize": 14,
    "ui.lineNumbers": "relative",
    "editor.transactionIdleMs": 500,
  },
});

vy.commands.registerCommand("user.uppercase-document", async () => {
  const active = await vy.editor.activeDocument();
  if (!active) {
    return;
  }

  const text = await vy.document.getText({ documentId: active.id });
  await vy.editor.edit({
    edits: [
      {
        range: {
          start: { line: 0, character: 0 },
          end: active.end,
        },
        text: text.toUpperCase(),
      },
    ],
  });

  await vy.notifications.show({ message: "Uppercased the document" });
});
