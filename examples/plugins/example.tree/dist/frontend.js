import { commands, fs, ui, views, workspace } from "vy-api";

// Cache of directory entries: uri -> DirEntry[]
const treeCache = new Map();
// Set of expanded directory URIs
const expandedDirs = new Set();
let isInitialized = false;

function getRootUri() {
  if (workspace.rootUri) {
    return workspace.rootUri;
  }
  return ".";
}

async function loadDirectory(dirUri) {
  try {
    const entries = await fs.readDir(dirUri);
    // Sort: directories first, then alphabetical by name
    entries.sort((a, b) => {
      const aDir = Boolean(a.is_directory || a.isDirectory);
      const bDir = Boolean(b.is_directory || b.isDirectory);
      if (aDir !== bDir) {
        return aDir ? -1 : 1;
      }
      return a.name.localeCompare(b.name);
    });
    treeCache.set(dirUri, entries);
  } catch (err) {
    console.error(`Failed to read directory "${dirUri}": ${err}`);
    treeCache.set(dirUri, []);
  }
}

async function refreshAll() {
  const root = getRootUri();
  expandedDirs.add(root);
  await loadDirectory(root);
  for (const dir of expandedDirs) {
    if (dir !== root) {
      await loadDirectory(dir);
    }
  }
  views.update("example.tree");
}

function renderTree(dirUri, depth = 0) {
  const entries = treeCache.get(dirUri) || [];
  const nodes = [];

  for (const entry of entries) {
    const indent = "  ".repeat(depth);
    const isDir = Boolean(entry.is_directory || entry.isDirectory);
    if (isDir) {
      const isExpanded = expandedDirs.has(entry.uri);
      const icon = isExpanded ? "▾ 📁 " : "▸ 📁 ";
      nodes.push(
        ui.button({
          id: `dir_${entry.uri}`,
          label: `${indent}${icon}${entry.name}`,
          command: `example.tree.toggle_dir ${entry.uri}`,
        })
      );
      if (isExpanded) {
        const children = renderTree(entry.uri, depth + 1);
        for (const child of children) {
          nodes.push(child);
        }
      }
    } else {
      nodes.push(
        ui.button({
          id: `file_${entry.uri}`,
          label: `${indent}  📄 ${entry.name}`,
          command: `vy.editor.open ${entry.uri}`,
        })
      );
    }
  }

  return nodes;
}

function renderView() {
  const root = getRootUri();
  const rootName = workspace.name || "PROJECT";

  const header = ui.row({ gap: "sm" }, [
    ui.text({ text: `📁 ${rootName}` }),
    ui.button({
      id: "tree_refresh_btn",
      label: "↻",
      command: "example.tree.refresh",
    }),
  ]);

  const items = renderTree(root);
  const body =
    items.length > 0
      ? items
      : [ui.text({ text: "  (Empty folder or loading...)", color: "muted" })];

  return ui.column({ gap: "sm" }, [
    header,
    ui.separator(),
    ui.column(body),
  ]);
}

// Register the view
views.register("example.tree", {
  kind: "ui",
  title: "Files",
  render: renderView,
});

// Commands
commands.registerCommand("example.tree.toggle", async () => {
  if (!isInitialized) {
    isInitialized = true;
    await refreshAll();
  }
  await views.toggle("example.tree", { direction: "left" });
});

commands.registerCommand("example.tree.open", async () => {
  if (!isInitialized) {
    isInitialized = true;
    await refreshAll();
  }
  await views.open("example.tree", { direction: "left" });
});

commands.registerCommand("example.tree.refresh", async () => {
  await refreshAll();
});

commands.registerCommand("example.tree.toggle_dir", async (args) => {
  const dirUri = typeof args === "string" ? args : args?.raw || args?.uri;
  if (!dirUri) return;
  if (expandedDirs.has(dirUri)) {
    expandedDirs.delete(dirUri);
  } else {
    expandedDirs.add(dirUri);
    if (!treeCache.has(dirUri)) {
      await loadDirectory(dirUri);
    }
  }
  views.update("example.tree");
});

// Pre-load root directory on activation
refreshAll().catch((err) => {
  console.error(`Explorer initial load failed: ${err}`);
});
