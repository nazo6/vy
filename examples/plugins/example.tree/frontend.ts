import type { DirEntry, WidgetNode, WorkspaceInfoReply } from "vy-api";

declare global {
  namespace Vy {
    interface CommandMap {
      "example.tree.toggle_dir": string;
    }
  }
}

const treeCache = new Map<string, DirEntry[]>();
const expandedDirs = new Set<string>();
let workspaceInfo: WorkspaceInfoReply = {
  rootUri: ".",
  name: "PROJECT",
  folders: [],
  trusted: false,
};
let isInitialized = false;

async function loadWorkspace(): Promise<void> {
  workspaceInfo = await vy.workspace.info();
}

async function loadDirectory(dirUri: string): Promise<void> {
  try {
    const { entries } = await vy.fs.readDir({ uri: dirUri });
    entries.sort((a, b) => {
      if (a.is_directory !== b.is_directory) {
        return a.is_directory ? -1 : 1;
      }
      return a.name.localeCompare(b.name);
    });
    treeCache.set(dirUri, entries);
  } catch (error) {
    treeCache.set(dirUri, []);
    await vy.log.write({
      channel: "example.tree",
      level: "error",
      message: `Failed to read directory "${dirUri}"`,
      data: String(error),
    });
  }
}

function renderTree(dirUri: string, depth = 0): WidgetNode[] {
  const entries = treeCache.get(dirUri) ?? [];
  const nodes: WidgetNode[] = [];

  for (const entry of entries) {
    const indent = "  ".repeat(depth);
    if (entry.is_directory) {
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

function renderView(): WidgetNode {
  const root = workspaceInfo.rootUri ?? ".";
  const rootName = workspaceInfo.name ?? "PROJECT";

  const header: WidgetNode = {
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
  const body: WidgetNode[] =
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

async function refreshAll(): Promise<void> {
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
    await refreshAll();
    isInitialized = true;
  }
  await vy.views.toggle({ view: "example.tree", direction: "left" });
});

vy.commands.registerCommand("example.tree.open", async () => {
  if (!isInitialized) {
    await refreshAll();
    isInitialized = true;
  }
  await vy.views.open({ view: "example.tree", direction: "left" });
});

vy.commands.registerCommand("example.tree.refresh", async () => {
  await refreshAll();
});

vy.commands.registerCommand("example.tree.toggle_dir", async (dirUri) => {
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

try {
  await refreshAll();
  isInitialized = true;
} catch (error) {
  await vy.log.write({
    channel: "example.tree",
    level: "error",
    message: "Explorer initial load failed",
    data: String(error),
  });
}
