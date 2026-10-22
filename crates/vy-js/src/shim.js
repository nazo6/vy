// `vy-api` shim.
//
// The host installs `globalThis.__vy_host_call(method, paramsJson)`. This file
// turns that bridge into the asynchronous namespaces documented in
// `docs/api.md`. Values cross the boundary as JSON, and positions are UTF-16
// offsets as JavaScript counts them.
//
// Keep this file plain JavaScript: QuickJS loads it as an ES module without a
// build step, so no type annotations may appear here.

const hostCall = globalThis.__vy_host_call;
const env = globalThis.__vy_env || {};

function parseError(reply) {
  const error = new Error(reply.error.message);
  error.code = reply.error.code;
  return error;
}

// Calls the host and returns the value. Host errors become exceptions with a
// stable `code`, as `docs/api.md` §20 describes.
function call(method, params) {
  const raw = hostCall(method, JSON.stringify(params === undefined ? null : params));
  const reply = JSON.parse(raw);
  if (reply.error) {
    throw parseError(reply);
  }
  return reply.value;
}

const commandHandlers = new Map();
const configListeners = [];

// Called by the host when a command registered from this script runs.
globalThis.__vy_run_command = (id, argsJson) => {
  const handler = commandHandlers.get(id);
  if (!handler) {
    throw new Error(`unknown command: ${id}`);
  }
  const args = argsJson ? JSON.parse(argsJson) : undefined;
  return Promise.resolve(handler(args));
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

export const commands = {
  registerCommand(id, handler) {
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
  },

  execute: async (id, args) => call("commands.execute", { id, args: args ?? null }),

  getCommands: async () => call("commands.getCommands", {}),
};

export const keymap = {
  bind(keys, target, options) {
    if (typeof target !== "string") {
      throw new TypeError("keymap.bind needs a command id or a registered function");
    }
    const settings = options || {};
    const when = settings.when ?? (settings.mode ? `${settings.mode}Mode` : null);
    call("keymap.bind", { keys, command: target, when });
    return {
      dispose() {
        call("keymap.unbind", { keys });
      },
    };
  },

  unbind(keys) {
    call("keymap.unbind", { keys });
  },

  setContext(key, value) {
    call("keymap.setContext", { key, value });
  },
};

export const config = {
  get: async (key) => call("config.get", { key }),

  // Accepts the object form used by `init.ts` and the `(key, value)` form from
  // `docs/api.md`. Both write to the memory layer only.
  set(first, second) {
    const values = typeof first === "string" ? { [first]: second } : (first || {});
    return call("config.set", { values });
  },

  inspect: async (key) => call("config.inspect", { key }),

  onDidChange(key, listener) {
    configListeners.push({ key, listener });
    return {
      dispose() {
        const index = configListeners.findIndex((entry) => entry.listener === listener);
        if (index >= 0) {
          configListeners.splice(index, 1);
        }
      },
    };
  },
};

export const editor = {
  activeDocument: async () => {
    const info = call("editor.activeDocument", {});
    return info ? makeDocument(info) : null;
  },

  documents: async () => call("editor.documents", {}).map(makeDocument),

  mode: async () => call("editor.mode", {}),

  setMode: async (mode) => call("editor.setMode", { mode }),

  selections: async () => call("editor.selections", {}),

  setSelections: async (selections) => call("editor.setSelections", { selections }),

  edit(builder) {
    if (typeof builder !== "function") {
      throw new TypeError("editor.edit needs a builder function");
    }
    const edits = [];
    builder({
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
  },

  undo: async () => call("editor.undo", {}),

  redo: async () => call("editor.redo", {}),
};

export const workspace = {
  rootUri: env.workspace?.rootUri ?? null,
  name: env.workspace?.name ?? null,
  trusted: env.workspace?.trusted ?? false,

  folders: async () => env.workspace?.folders ?? [],

  textDocuments: async () => call("workspace.textDocuments", {}).map(makeDocument),
};

export const notifications = {
  show: async (message) => {
    const options = typeof message === "string" ? { message } : (message || {});
    return call("notifications.show", options);
  },
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

export const log = { createChannel };

export const remote = {
  isRemote: env.remote?.isRemote ?? false,
  name: env.remote?.name ?? "local",
  os: env.remote?.os ?? null,
  pathSeparator: env.remote?.pathSeparator ?? "/",
};

let nextStatuslineItemId = 1;

export const ui = {
  statusline: {
    createItem(options) {
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
    },
  },
};

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
