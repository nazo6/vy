// Cache of directory entries: uri -> DirEntry[]
const treeCache = new Map();
// Set of expanded directory URIs
const expandedDirs = new Set();
let workspaceInfo = { rootUri: ".", name: "PROJECT" };
let isInitialized = false;

async function loadWorkspace() {
  workspaceInfo = await vy.workspace.info();
  workspaceInfo.rootUri ??= ".";
  workspaceInfo.name ??= "PROJECT";
}

async function loadDirectory(dirUri) {
  try {
    const { entries } = await vy.fs.readDir({ uri: dirUri });
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

function renderTree(dirUri, depth = 0) {
  const entries = treeCache.get(dirUri) || [];
  const nodes = [];

  for (const entry of entries) {
    const indent = "  ".repeat(depth);
    const isDir = Boolean(entry.is_directory || entry.isDirectory);
    if (isDir) {
      const isExpanded = expandedDirs.has(entry.uri);
      const icon = isExpanded ? "▾ 📁 " : "▸ 📁 ";
      nodes.push({
        type: "button",
        id: `dir_${entry.uri}`,
        label: `${indent}${icon}${entry.name}`,
        command: `example.tree.toggle_dir ${entry.uri}`,
      });
      if (isExpanded) {
        nodes.push(...renderTree(entry.uri, depth + 1));
      }
    } else {
      nodes.push({
        type: "button",
        id: `file_${entry.uri}`,
        label: `${indent}  📄 ${entry.name}`,
        command: `vy.editor.open ${entry.uri}`,
      });
    }
  }

  return nodes;
}

function renderView() {
  const root = workspaceInfo.rootUri ?? ".";
  const rootName = workspaceInfo.name ?? "PROJECT";

  const header = {
    type: "container",
    kind: "row",
    gap: "sm",
    children: [
      { type: "text", text: `📁 ${rootName}` },
      {
        type: "button",
        id: "tree_refresh_btn",
        label: "↻",
        command: "example.tree.refresh",
      },
    ],
  };

  const items = renderTree(root);
  const body =
    items.length > 0
      ? items
      : [{ type: "text", text: "  (Empty folder or loading...)", color: "muted" }];

  return {
    type: "container",
    kind: "column",
    gap: "sm",
    children: [
      header,
      { type: "separator" },
      { type: "container", kind: "column", children: body },
    ],
  };
}

async function refreshAll() {
  await loadWorkspace();
  const root = workspaceInfo.rootUri ?? ".";
  expandedDirs.add(root);
  await loadDirectory(root);
  for (const dir of expandedDirs) {
    if (dir !== root) {
      await loadDirectory(dir);
    }
  }
  await vy.views.render({ id: "example.tree", tree: renderView() });
}

vy.commands.registerCommand("example.tree.toggle", async () => {
  if (!isInitialized) {
    isInitialized = true;
    await refreshAll();
  }
  await vy.views.toggle({ view: "example.tree", direction: "left" });
});

vy.commands.registerCommand("example.tree.open", async () => {
  if (!isInitialized) {
    isInitialized = true;
    await refreshAll();
  }
  await vy.views.open({ view: "example.tree", direction: "left" });
});

vy.commands.registerCommand("example.tree.refresh", async () => {
  await refreshAll();
});

vy.commands.registerCommand("example.tree.toggle_dir", async (args) => {
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
  await vy.views.render({ id: "example.tree", tree: renderView() });
});

refreshAll().catch((err) => {
  console.error(`Explorer initial load failed: ${err}`);
});
