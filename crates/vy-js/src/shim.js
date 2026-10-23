// `vy-api` shim.
//
// The host installs three globals before a script runs:
//
//   __vy_host_call(method, paramsJson)  the blocking bridge to the client
//   __vy_env                            constants such as workspace.rootUri
//   __vy_spec                           HOST_METHODS from vy-api-schema
//
// Most methods are generated from `__vy_spec`: the caller passes one object and
// it is forwarded unchanged, so adding an API never needs an edit here. The rest
// need JavaScript state (a handler map, a builder, a handle returned to the
// script) and are written by hand below.
//
// Keep this file plain JavaScript: QuickJS loads it as an ES module without a
// build step, so no type annotations may appear here.

const hostCall = globalThis.__vy_host_call;
const env = globalThis.__vy_env || {};
const spec = globalThis.__vy_spec || [];

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

export const commands = {};
export const keymap = {};
export const config = {};
export const editor = {};
export const notifications = {};
export const log = {};
export const views = {};

// `workspace` is mostly constants; `textDocuments` is added below.
export const workspace = {
  rootUri: env.workspace?.rootUri ?? null,
  name: env.workspace?.name ?? null,
  trusted: env.workspace?.trusted ?? false,
  folders: () => env.workspace?.folders ?? [],
};

export const remote = {
  isRemote: env.remote?.isRemote ?? false,
  name: env.remote?.name ?? "local",
  os: env.remote?.os ?? null,
  pathSeparator: env.remote?.pathSeparator ?? "/",
};

export const ui = {
  picker: {},
  statusline: {},
  gutter: {},
  hover: {},
};

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

// --- generated forwarders ---------------------------------------------------

const namespaces = { commands, keymap, config, editor, workspace, notifications, log, ui, views };

for (const entry of spec) {
  if (entry.manual) {
    continue;
  }
  const segments = entry.method.split(".");
  let target = namespaces[segments[0]];
  if (!target) {
    continue;
  }
  for (let index = 1; index < segments.length - 1; index += 1) {
    const name = segments[index];
    if (!target[name]) {
      target[name] = {};
    }
    target = target[name];
  }
  target[segments[segments.length - 1]] = entry.hasParams
    ? async (params) => call(entry.method, params ?? {})
    : async () => call(entry.method, {});
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

// --- workspace --------------------------------------------------------------

workspace.textDocuments = async () => call("workspace.textDocuments", {}).map(makeDocument);

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
