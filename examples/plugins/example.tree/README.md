# File Explorer Plugin (`example.tree`)

An interactive sidebar file explorer built for Vy using native declarative UI widgets.

This example uses the source form: `plugin.json` points at `frontend.ts`, so Vy
loads the TypeScript directly without a build step.

## Features

- Tree view of directories and files using `fs.readDir`.
- Expand and collapse folders on click.
- Open files in the adjacent editor pane on click (`vy.editor.open`).
- Left sidebar toggle command (`example.tree.toggle`).
- Quick reload / refresh (`example.tree.refresh`).
