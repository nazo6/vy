// The runtime `vy-api` module served to scripts.
//
// The host installs two globals before a script runs:
//
//   __vy_host_call(method, paramsJson)  the blocking bridge to the client
//   __vy_emit(topic, payloadJson)       host-to-script events
//
// Method namespaces are proxies. Reading a property that starts with `on`
// returns an event subscription; any other property returns a forwarder for
// `<namespace>.<property>`. Command registration and execution are explicit
// wrappers because they need JS-only types and callbacks.

import type {
  CommandsApi,
  ConfigApi,
  ContextApi,
  Disposable,
  DocumentApi,
  EditorApi,
  FsApi,
  KeymapApi,
  LogApi,
  NotificationsApi,
  ProcessApi,
  UiApi,
  UiGutterApi,
  UiHoverApi,
  UiPickerApi,
  UiStatuslineApi,
  ViewsApi,
  WorkspaceApi,
} from "../../../packages/vy-api/api-base";
import type { BuiltinCommandId, BuiltinCommandMap } from "../../../packages/vy-api/types";

type HostCall = (method: string, paramsJson: string) => string;

interface HostError {
  code: string;
  message: string;
}

interface HostReply {
  error?: HostError;
  value?: unknown;
}

interface HostGlobals {
  __vy_host_call: HostCall;
  __vy_emit?: (topic: string, payloadJson?: string) => void;
  __vy_run_command?: (id: string, argsJson?: string) => unknown;
}

const globals = globalThis as unknown as HostGlobals;
const hostCall = globals.__vy_host_call;

/** Augment this interface to type the arguments of your own commands. */
export interface CommandMap {}

// Calls the host and returns the value. Host errors become exceptions with a
// stable `code`, as `docs/api.md` §20 describes.
function call(method: string, params?: unknown): unknown {
  const raw = hostCall(method, JSON.stringify(params === undefined ? null : params));
  const reply = JSON.parse(raw) as HostReply;
  if (reply.error) {
    const error = new Error(reply.error.message) as Error & { code: string };
    error.code = reply.error.code;
    throw error;
  }
  return reply.value;
}

// --- events ---------------------------------------------------------------

const listeners = new Map<string, Set<(payload: unknown) => void>>();

function subscribe(topic: string, listener: (payload: unknown) => void): Disposable {
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
globals.__vy_emit = (topic: string, payloadJson?: string) => {
  const payload = payloadJson ? JSON.parse(payloadJson) : undefined;
  for (const listener of listeners.get(topic) ?? []) {
    try {
      listener(payload);
    } catch (error) {
      console.error(error);
    }
  }
};

// --- namespaces -----------------------------------------------------------

function methods<T>(path: string): T {
  return new Proxy({} as Record<string, unknown>, {
    get(target, name) {
      if (typeof name !== "string" || name === "then") {
        return undefined;
      }
      if (!Object.prototype.hasOwnProperty.call(target, name)) {
        target[name] = name.startsWith("on")
          ? (listener: unknown) =>
              subscribe(`${path}.${name}`, listener as (payload: unknown) => void)
          : async (params?: unknown) => call(`${path}.${name}`, params ?? {});
      }
      return target[name];
    },
  }) as T;
}

// --- commands -------------------------------------------------------------

const commandHandlers = new Map<string, (args?: unknown) => unknown>();

// Called by the host when a command registered from this script runs.
globals.__vy_run_command = (id: string, argsJson?: string) => {
  const handler = commandHandlers.get(id);
  if (!handler) {
    throw new Error(`unknown command: ${id}`);
  }
  const args = argsJson ? JSON.parse(argsJson) : undefined;
  return Promise.resolve(handler(args));
};

type RegisterHandler<K extends string> = K extends keyof BuiltinCommandMap
  ? never
  : K extends keyof CommandMap
    ? (args: CommandMap[K]) => unknown
    : (args?: unknown) => unknown;

function registerCommand<const K extends string>(
  id: K,
  handler: RegisterHandler<K>,
): Disposable {
  const callback = handler as (args?: unknown) => unknown;
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

type ExecuteParams<K extends string> = K extends keyof BuiltinCommandMap
  ? { id: K; args?: BuiltinCommandMap[K] }
  : K extends keyof CommandMap
    ? undefined extends CommandMap[K]
      ? { id: K; args?: CommandMap[K] }
      : { id: K; args: CommandMap[K] }
    : { id: K; args?: unknown };

function execute<const K extends string>(params: ExecuteParams<K>): Promise<unknown> {
  return Promise.resolve(call("commands.execute", params));
}

export const commands = Object.assign(methods<CommandsApi>("commands"), {
  registerCommand,
  execute,
});
export const config = methods<ConfigApi>("config");
export const context = methods<ContextApi>("context");
export const document = methods<DocumentApi>("document");
export const editor = methods<EditorApi>("editor");
export const fs = methods<FsApi>("fs");

const keymapBase = methods<KeymapApi>("keymap");
function bind<K extends string & (BuiltinCommandId | keyof CommandMap)>(
  params: { keys: string; command: K; when?: string | null },
): Promise<void>;
function bind(params: {
  keys: string;
  command: string;
  when?: string | null;
}): Promise<void>;
function bind(params: {
  keys: string;
  command: string;
  when?: string | null;
}): Promise<void> {
  call("keymap.bind", params);
  return Promise.resolve();
}
export const keymap = Object.assign(keymapBase, { bind });

export const log = methods<LogApi>("log");
export const notifications = methods<NotificationsApi>("notifications");
export const process = methods<ProcessApi>("process");
export const views = methods<ViewsApi>("views");
export const workspace = methods<WorkspaceApi>("workspace");

export const ui = Object.assign(methods<UiApi>("ui"), {
  gutter: methods<UiGutterApi>("ui.gutter"),
  hover: methods<UiHoverApi>("ui.hover"),
  picker: methods<UiPickerApi>("ui.picker"),
  statusline: methods<UiStatuslineApi>("ui.statusline"),
});
