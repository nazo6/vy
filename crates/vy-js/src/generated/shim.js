// The runtime API installed as the global `vy`.
//
// The host installs three globals before a script runs:
//
//   vy                                 the API object defined here
//   __vy_host_call(method, paramsJson)  the blocking bridge to the client
//   __vy_emit(topic, payloadJson)       host-to-script events
//
// Method namespaces are proxies. Reading a property that starts with `on`
// returns an event subscription; any other property returns a forwarder for
// `<namespace>.<property>`. Command registration and execution are explicit
// wrappers because they need JS-only types and callbacks.
const globals = globalThis;
const hostCall = globals.__vy_host_call;
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
// --- events ---------------------------------------------------------------
const listeners = new Map();
function subscribe(topic, listener) {
    if (typeof listener !== "function") {
        throw new TypeError(`an event listener must be a function: ${topic}`);
    }
    if (!listeners.has(topic)) {
        listeners.set(topic, new Set());
    }
    listeners.get(topic)?.add(listener);
    return {
        dispose() {
            listeners.get(topic)?.delete(listener);
        },
    };
}
// Called by the host when it emits an event.
globals.__vy_emit = (topic, payloadJson) => {
    const payload = payloadJson ? JSON.parse(payloadJson) : undefined;
    for (const listener of listeners.get(topic) ?? []) {
        try {
            listener(payload);
        }
        catch (error) {
            console.error(error);
        }
    }
};
// --- namespaces -----------------------------------------------------------
function methods(path) {
    return new Proxy({}, {
        get(target, name) {
            if (typeof name !== "string" || name === "then") {
                return undefined;
            }
            if (!Object.prototype.hasOwnProperty.call(target, name)) {
                target[name] = name.startsWith("on")
                    ? (listener) => subscribe(`${path}.${name}`, listener)
                    : async (params) => call(`${path}.${name}`, params ?? {});
            }
            return target[name];
        },
    });
}
// --- commands -------------------------------------------------------------
const commandHandlers = new Map();
// Called by the host when a command registered from this script runs.
globals.__vy_run_command = (id, argsJson) => {
    const handler = commandHandlers.get(id);
    if (!handler) {
        throw new Error(`unknown command: ${id}`);
    }
    const args = argsJson ? JSON.parse(argsJson) : undefined;
    return Promise.resolve(handler(args));
};
function registerCommand(id, handler) {
    const callback = handler;
    if (typeof callback !== "function") {
        throw new TypeError("commands.registerCommand needs a handler function");
    }
    call("commands.registerCommand", { id });
    commandHandlers.set(id, callback);
    return {
        dispose() {
            commandHandlers.delete(id);
        },
    };
}
function execute(params) {
    return Promise.resolve(call("commands.execute", params));
}
export const commands = Object.assign(methods("commands"), {
    registerCommand,
    execute,
});
export const config = methods("config");
export const context = methods("context");
export const document = methods("document");
export const editor = methods("editor");
export const fs = methods("fs");
const keymapBase = methods("keymap");
function bind(params) {
    call("keymap.bind", params);
    return Promise.resolve();
}
export const keymap = Object.assign(keymapBase, { bind });
export const log = methods("log");
export const notifications = methods("notifications");
export const process = methods("process");
export const views = methods("views");
export const workspace = methods("workspace");
export const ui = Object.assign(methods("ui"), {
    gutter: methods("ui.gutter"),
    hover: methods("ui.hover"),
    picker: methods("ui.picker"),
    statusline: methods("ui.statusline"),
});
// The host evaluates this module before any script. The exports stay for type
// generation; scripts use the global object.
const api = {
    commands,
    config,
    context,
    document,
    editor,
    fs,
    keymap,
    log,
    notifications,
    process,
    ui,
    views,
    workspace,
};
globalThis.vy = api;
