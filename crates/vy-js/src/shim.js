// `vy-api` shim.
//
// The host installs two globals before a script runs:
//
//   __vy_host_call(method, paramsJson)  the blocking bridge to the client
//   __vy_env                            constants such as workspace.rootUri
//
// Method namespaces are proxies: reading a property that is not hand-written
// sugar returns a forwarder for `<namespace>.<property>`, so adding a method in
// Rust needs no edit here. The hand-written functions below keep JavaScript
// state (a handler map, a builder, a handle returned to the script).
//
// Keep this file plain JavaScript: QuickJS loads it as an ES module without a
// build step, so no type annotations may appear here.

const hostCall = globalThis.__vy_host_call;
const env = globalThis.__vy_env || {};

// Calls the host and returns the value. Host errors become exceptions with a
// stable `code`, as `docs/api.md` §20 describes.
function call(method, params) {
  const raw = hostCall(method, JSON.stringify(params === undefined ? null : params));
  const reply = JSON.parse(raw);
  if (reply.error) {
    const error = new Error(reply.error.message);
    error.code = reply.error.code;
    throw error;
  }
  return reply.value;
}

// A namespace whose unknown properties are host forwarders.
function methods(path) {
  return new Proxy(
    {},
    {
      get(target, name) {
        if (typeof name !== "string" || name === "then") {
          return undefined;
        }
        if (!Object.prototype.hasOwnProperty.call(target, name)) {
          target[name] = async (params) => call(`${path}.${name}`, params ?? {});
        }
        return target[name];
      },
    }
  );
}

export const commands = methods("commands");
export const keymap = methods("keymap");
export const config = methods("config");
export const editor = methods("editor");
export const notifications = methods("notifications");
export const log = methods("log");
export const views = methods("views");
export const fs = methods("fs");
export const process = methods("process");

// `workspace` is mostly constants; `textDocuments` is added below.
export const workspace = methods("workspace");
workspace.rootUri = env.workspace?.rootUri ?? null;
workspace.name = env.workspace?.name ?? null;
workspace.trusted = env.workspace?.trusted ?? false;
workspace.folders = () => env.workspace?.folders ?? [];

export const remote = {
  isRemote: env.remote?.isRemote ?? false,
  name: env.remote?.name ?? "local",
  os: env.remote?.os ?? null,
  pathSeparator: env.remote?.pathSeparator ?? "/",
};

export const ui = methods("ui");
ui.picker = methods("ui.picker");
ui.statusline = methods("ui.statusline");
ui.gutter = methods("ui.gutter");
ui.hover = methods("ui.hover");

// Turns a host document into the object scripts use. Text and positions are in
// UTF-16 code units, as JavaScript counts them.
function makeDocument(info) {
  return {
    id: info.id,
    uri: info.uri,
    languageId: info.languageId,
    version: info.version,
    lineCount: info.lineCount,
    end: info.end,
    getText: async (range) =>
      call("document.getText", { documentId: info.id, range: range ?? null }),
    lineAt: async (line) => call("document.lineAt", { documentId: info.id, line }),
    positionAt: async (offset) =>
      call("document.positionAt", { documentId: info.id, offset }),
    offsetAt: async (position) =>
      call("document.offsetAt", { documentId: info.id, position }),
    fullRange: () => ({
      start: { line: 0, character: 0 },
      end: { line: info.end.line, character: info.end.character },
    }),
  };
}

// --- commands ---------------------------------------------------------------

const commandHandlers = new Map();

// Called by the host when a command registered from this script runs.
globalThis.__vy_run_command = (id, argsJson) => {
  const handler = commandHandlers.get(id);
  if (!handler) {
    throw new Error(`unknown command: ${id}`);
  }
  const args = argsJson ? JSON.parse(argsJson) : undefined;
  return Promise.resolve(handler(args));
};

commands.registerCommand = (id, handler) => {
  if (typeof handler !== "function") {
    throw new TypeError("commands.registerCommand needs a handler function");
  }
  call("commands.registerCommand", { id });
  commandHandlers.set(id, handler);
  return {
    dispose() {
      commandHandlers.delete(id);
    },
  };
};

commands.execute = async (id, args) => {
  if (commandHandlers.has(id)) {
    return await commandHandlers.get(id)(args ?? null);
  }
  return call("commands.execute", { id, args: args ?? null });
};

// --- keymap -----------------------------------------------------------------

keymap.bind = (keys, command, options) => {
  if (typeof command !== "string") {
    throw new TypeError("keymap.bind needs a command id");
  }
  const settings = options || {};
  const when = settings.when ?? (settings.mode ? `${settings.mode}Mode` : null);
  call("keymap.bind", { keys, command, when });
  return {
    dispose() {
      call("keymap.unbind", { keys });
    },
  };
};

// --- config -----------------------------------------------------------------

const configListeners = [];
const activeDocumentListeners = [];
const selectionListeners = [];
const workspaceFolderListeners = [];

// Called by the host when the active editor/document changes.
globalThis.__vy_emit_active_editor_change = (docJson) => {
  const info = docJson ? JSON.parse(docJson) : null;
  const doc = info ? makeDocument(info) : null;
  for (const listener of activeDocumentListeners) {
    try {
      listener(doc);
    } catch (error) {
      console.error(`editor.onDidChangeActiveEditor listener failed: ${error}`);
    }
  }
};

// Called by the host when selections in the active editor change.
globalThis.__vy_emit_selection_change = (selectionsJson) => {
  const selections = JSON.parse(selectionsJson);
  for (const listener of selectionListeners) {
    try {
      listener({ selections });
    } catch (error) {
      console.error(`editor.onDidChangeSelection listener failed: ${error}`);
    }
  }
};

// Called by the host when workspace folders change.
globalThis.__vy_emit_workspace_folders_change = (foldersJson) => {
  const folders = JSON.parse(foldersJson);
  for (const listener of workspaceFolderListeners) {
    try {
      listener({ folders });
    } catch (error) {
      console.error(`workspace.onDidChangeWorkspaceFolders listener failed: ${error}`);
    }
  }
};

// Called by the host after a settings reload. Prefixes match nested keys.
globalThis.__vy_emit_config_change = (keysJson) => {
  const keys = JSON.parse(keysJson);
  for (const entry of configListeners) {
    if (keys.some((key) => key === entry.key || key.startsWith(`${entry.key}.`))) {
      try {
        entry.listener({ key: entry.key });
      } catch (error) {
        console.error(`config.onDidChange listener failed: ${error}`);
      }
    }
  }
};

// Accepts the object form used by `init.ts` and the `(key, value)` form from
// `docs/api.md`. Both write to the memory layer only.
config.set = (first, second) => {
  const values = typeof first === "string" ? { [first]: second } : (first || {});
  return call("config.set", { values });
};

// Hand-written so the declaration can return the type of a known key.
config.get = async (params) => call("config.get", params ?? {});

config.onDidChange = (key, listener) => {
  configListeners.push({ key, listener });
  return {
    dispose() {
      const index = configListeners.findIndex((entry) => entry.listener === listener);
      if (index >= 0) {
        configListeners.splice(index, 1);
      }
    },
  };
};

// --- editor -----------------------------------------------------------------

editor.activeDocument = async () => {
  const info = call("editor.activeDocument", {});
  return info ? makeDocument(info) : null;
};

editor.documents = async () => call("editor.documents", {}).map(makeDocument);

// Accepts a list of edits or a builder callback, as `docs/api.md` §4 describes.
editor.edit = (input) => {
  if (typeof input === "function") {
    const edits = [];
    input({
      insert(position, text) {
        edits.push({ range: { start: position, end: position }, text });
      },
      replace(range, text) {
        edits.push({ range, text });
      },
      delete(range) {
        edits.push({ range, text: "" });
      },
    });
    return call("editor.edit", { edits });
  }
  return call("editor.edit", { edits: input || [] });
};

editor.open = async (uri, options) => {
  if (!uri) {
    throw new TypeError("editor.open needs a uri string");
  }
  await commands.execute("vy.editor.open", {
    uri: String(uri),
    paneId: options?.paneId ?? null,
  });
};

editor.onDidChangeActiveDocument = (listener) => {
  activeDocumentListeners.push(listener);
  return {
    dispose() {
      const index = activeDocumentListeners.indexOf(listener);
      if (index >= 0) activeDocumentListeners.splice(index, 1);
    },
  };
};
editor.onDidChangeActiveEditor = editor.onDidChangeActiveDocument;

editor.onDidChangeSelection = (listener) => {
  selectionListeners.push(listener);
  return {
    dispose() {
      const index = selectionListeners.indexOf(listener);
      if (index >= 0) selectionListeners.splice(index, 1);
    },
  };
};

// --- workspace --------------------------------------------------------------

workspace.textDocuments = async () => call("workspace.textDocuments", {}).map(makeDocument);

workspace.onDidChangeWorkspaceFolders = (listener) => {
  workspaceFolderListeners.push(listener);
  return {
    dispose() {
      const index = workspaceFolderListeners.indexOf(listener);
      if (index >= 0) workspaceFolderListeners.splice(index, 1);
    },
  };
};
workspace.onDidChangeFolders = workspace.onDidChangeWorkspaceFolders;

// --- notifications, logs ----------------------------------------------------

notifications.show = async (message) => {
  const options = typeof message === "string" ? { message } : (message || {});
  return call("notifications.show", options);
};

function createChannel(name) {
  const write = (level, message, data) =>
    call("log.write", {
      channel: name,
      level,
      message: typeof message === "string" ? message : String(message),
      data: data ?? null,
    });

  return {
    info: async (message, data) => write("info", message, data),
    warn: async (message, data) => write("warn", message, data),
    error: async (message, data) => write("error", message, data),
  };
}

log.createChannel = createChannel;

// --- slots ------------------------------------------------------------------

let nextStatuslineItemId = 1;
let nextGutterItemId = 1;
let nextPickerId = 1;

ui.statusline.createItem = (options) => {
  if (!options || typeof options !== "object") {
    throw new TypeError("ui.statusline.createItem needs options object");
  }
  const id = options.id || `statusline_item_${nextStatuslineItemId++}`;
  let text = options.text || "";
  let color = options.color || null;
  let command = options.command || null;
  const alignment = options.alignment || "left";
  const priority = typeof options.priority === "number" ? options.priority : 0;

  call("ui.statusline.createItem", {
    id,
    alignment,
    priority,
    text,
    color,
    command,
  });

  return {
    get id() {
      return id;
    },
    get text() {
      return text;
    },
    set text(value) {
      text = String(value);
      call("ui.statusline.updateItem", { id, text });
    },
    get color() {
      return color;
    },
    set color(value) {
      color = value ? String(value) : null;
      call("ui.statusline.updateItem", { id, color });
    },
    get command() {
      return command;
    },
    set command(value) {
      command = value ? String(value) : null;
      call("ui.statusline.updateItem", { id, command });
    },
    show() {
      call("ui.statusline.updateItem", { id, visible: true });
    },
    hide() {
      call("ui.statusline.updateItem", { id, visible: false });
    },
    dispose() {
      call("ui.statusline.disposeItem", { id });
    },
  };
};

ui.gutter.createItem = (options) => {
  if (!options || typeof options !== "object") {
    throw new TypeError("ui.gutter.createItem needs options object");
  }
  const id = options.id || `gutter_item_${nextGutterItemId++}`;
  const line = typeof options.line === "number" ? options.line : 0;
  const text = options.text || " ";
  const color = options.color || null;

  call("ui.gutter.createItem", { id, line, text, color });
  return {
    id,
    dispose() {
      call("ui.gutter.disposeItem", { id });
    },
  };
};

ui.picker.show = (options) => {
  if (!options || typeof options !== "object") {
    throw new TypeError("ui.picker.show needs options object");
  }
  const id = String(nextPickerId++);
  const items = Array.isArray(options.items) ? options.items : [];
  const placeholder = options.placeholder || null;

  const reply = call("ui.picker.show", { id, items, placeholder });
  if (!reply || !reply.selectedId) {
    return Promise.resolve(undefined);
  }
  const found = items.find((item) => item.id === reply.selectedId);
  return Promise.resolve(found);
};

// --- views and widgets ------------------------------------------------------

const registeredViews = new Map();

views.register = (type, options) => {
  if (!type || typeof type !== "string") {
    throw new TypeError("views.register needs a string type");
  }
  registeredViews.set(type, options);
  if (options && typeof options.render === "function") {
    const tree = options.render();
    call("views.render", { id: type, tree });
  }
  return {
    dispose() {
      registeredViews.delete(type);
    },
  };
};

views.get = (type) => registeredViews.get(type);

views.open = async (viewId, options) => {
  if (!viewId || typeof viewId !== "string") {
    throw new TypeError("views.open needs a view string");
  }
  await commands.execute("vy.view.open", {
    view: viewId,
    direction: options?.direction ?? "left",
  });
};

views.close = async (viewId) => {
  if (!viewId || typeof viewId !== "string") {
    throw new TypeError("views.close needs a view string");
  }
  await commands.execute("vy.view.close", { view: viewId });
};

views.toggle = async (viewId, options) => {
  if (!viewId || typeof viewId !== "string") {
    throw new TypeError("views.toggle needs a view string");
  }
  await commands.execute("vy.view.toggle", {
    view: viewId,
    direction: options?.direction ?? "left",
  });
};

views.update = (viewId) => {
  if (!viewId || typeof viewId !== "string") {
    throw new TypeError("views.update needs a view string");
  }
  const reg = registeredViews.get(viewId);
  if (reg && typeof reg.render === "function") {
    const tree = reg.render();
    call("views.render", { id: viewId, tree });
  }
};

function widgetContainer(kind, options, children) {
  const opts = Array.isArray(options) ? {} : options || {};
  const kids = Array.isArray(options) ? options : children || [];
  return {
    type: "container",
    kind,
    id: opts.id ?? null,
    gap: opts.gap ?? null,
    children: kids,
  };
}

ui.column = (options, children) => widgetContainer("column", options, children);
ui.row = (options, children) => widgetContainer("row", options, children);
ui.stack = (options, children) => widgetContainer("stack", options, children);

ui.text = (options) => {
  const opts = typeof options === "string" ? { text: options } : options || {};
  return {
    type: "text",
    id: opts.id ?? null,
    text: opts.text ?? "",
    color: opts.color ?? null,
    font: opts.font ?? null,
  };
};

ui.button = (options) => {
  const opts = options || {};
  return {
    type: "button",
    id: opts.id ?? `btn_${Math.random().toString(36).slice(2)}`,
    label: opts.label ?? "",
    command: opts.command ?? "",
    variant: opts.variant ?? null,
  };
};

ui.textInput = (options) => {
  const opts = options || {};
  return {
    type: "textInput",
    id: opts.id ?? `input_${Math.random().toString(36).slice(2)}`,
    value: opts.value ?? "",
    placeholder: opts.placeholder ?? null,
  };
};

ui.separator = (options) => {
  const opts = options || {};
  return {
    type: "separator",
    id: opts.id ?? null,
  };
};

// --- file system ------------------------------------------------------------

fs.readFile = async (uri) => {
  const res = await call("fs.readFile", { uri: String(uri) });
  return new Uint8Array(res.bytes);
};

fs.readTextFile = async (uri) => {
  const res = await call("fs.readTextFile", { uri: String(uri) });
  return res.text;
};

fs.writeFile = async (uri, data) => {
  let params;
  if (typeof data === "string") {
    params = { uri: String(uri), text: data };
  } else if (data instanceof Uint8Array || Array.isArray(data)) {
    params = { uri: String(uri), bytes: Array.from(data) };
  } else {
    params = { uri: String(uri), text: String(data) };
  }
  await call("fs.writeFile", params);
};

fs.readDir = async (uri) => {
  const res = await call("fs.readDir", { uri: String(uri) });
  return res.entries;
};

fs.stat = async (uri) => {
  const res = await call("fs.stat", { uri: String(uri) });
  return res.stat;
};

fs.exists = async (uri) => {
  const res = await call("fs.exists", { uri: String(uri) });
  return res.exists;
};

fs.createDir = async (uri, options) => {
  await call("fs.createDir", { uri: String(uri), recursive: options?.recursive ?? false });
};

fs.remove = async (uri, options) => {
  await call("fs.remove", { uri: String(uri), recursive: options?.recursive ?? false });
};

fs.rename = async (from, to) => {
  await call("fs.rename", { from: String(from), to: String(to) });
};

fs.copy = async (from, to) => {
  await call("fs.copy", { from: String(from), to: String(to) });
};

// --- process ----------------------------------------------------------------

const processStdoutListeners = new Map();
const processStderrListeners = new Map();
const processExitListeners = new Map();

globalThis.__vy_emit_process_data = (json) => {
  const { pid, channel, data } = JSON.parse(json);
  const listeners =
    channel === "stderr"
      ? processStderrListeners.get(pid)
      : processStdoutListeners.get(pid);
  if (listeners) {
    for (const cb of listeners) {
      try {
        cb(data);
      } catch (e) {
        console.error(e);
      }
    }
  }
};

globalThis.__vy_emit_process_exit = (json) => {
  const { pid, exitCode } = JSON.parse(json);
  const listeners = processExitListeners.get(pid);
  if (listeners) {
    for (const cb of listeners) {
      try {
        cb(exitCode);
      } catch (e) {
        console.error(e);
      }
    }
  }
  processStdoutListeners.delete(pid);
  processStderrListeners.delete(pid);
  processExitListeners.delete(pid);
};

process.exec = async (command, args, options) => {
  return call("process.exec", {
    command,
    args: args ?? [],
    cwd: options?.cwd ?? null,
    env: options?.env ?? null,
  });
};

process.spawn = async (command, args, options) => {
  const res = await call("process.spawn", {
    command,
    args: args ?? [],
    cwd: options?.cwd ?? null,
    env: options?.env ?? null,
  });
  const pid = res.pid;
  return {
    pid,
    stdin: {
      write: async (data) => {
        await call("process.writeStdin", { pid, data: String(data) });
      },
    },
    kill: async (signal) => {
      await call("process.kill", { pid, signal: signal ?? null });
    },
    onStdout: (listener) => {
      if (!processStdoutListeners.has(pid)) {
        processStdoutListeners.set(pid, new Set());
      }
      processStdoutListeners.get(pid).add(listener);
      return {
        dispose() {
          processStdoutListeners.get(pid)?.delete(listener);
        },
      };
    },
    onStderr: (listener) => {
      if (!processStderrListeners.has(pid)) {
        processStderrListeners.set(pid, new Set());
      }
      processStderrListeners.get(pid).add(listener);
      return {
        dispose() {
          processStderrListeners.get(pid)?.delete(listener);
        },
      };
    },
    onExit: (listener) => {
      if (!processExitListeners.has(pid)) {
        processExitListeners.set(pid, new Set());
      }
      processExitListeners.get(pid).add(listener);
      return {
        dispose() {
          processExitListeners.get(pid)?.delete(listener);
        },
      };
    },
  };
};

// --- plugin context ---------------------------------------------------------

function covers(declared, requested) {
  if (declared === requested) {
    return true;
  }
  return requested.startsWith(`${declared}:`) || declared.startsWith(`${requested}:`);
}

export const context = {
  pluginId: env.pluginId ?? "init",
  version: env.version ?? "0.1.0",
  capabilities: {
    has(name) {
      return (env.capabilities ?? []).some((declared) => covers(declared, name));
    },
  },
  log: createChannel("init"),
};
