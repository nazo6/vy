// Type definitions for the `vy-api` module.
//
// The runtime module is served by the QuickJS host; this file only describes it.
// Position and range types come from the Rust crates in `../vy-api-types`, so a
// protocol change shows up here after `cargo test export_bindings`.
//
// Phase 2 exposes a subset of `docs/api.md`. APIs that are not listed here are
// not available to `init.ts` yet.

import type { Position, Range, Selection, TextEdit, Uri } from "../vy-api-types";

export type { ErrorCode, Position, Range, Selection, TextEdit, Uri } from "../vy-api-types";

/** Something that can be released when a script reloads. */
export interface Disposable {
  dispose(): void;
}

/** The mode reported by `editor.mode()` and accepted by `editor.setMode()`. */
export type Mode =
  | "normal"
  | "insert"
  | "select"
  | "select_line"
  | "select_block";

/** One layer's value for a setting, as `config.inspect` reports it. */
export interface Inspect {
  default: unknown;
  user: unknown;
  project: unknown;
  memory: unknown;
  effective: unknown;
}

/** The open document. Text and positions are in UTF-16 code units. */
export interface Document {
  readonly id: number;
  readonly uri: Uri;
  readonly languageId: string;
  readonly version: number;
  readonly lineCount: number;
  readonly end: Position;
  getText(range?: Range): Promise<string>;
  lineAt(line: number): Promise<string>;
  positionAt(offset: number): Promise<Position>;
  offsetAt(position: Position): Promise<number>;
  fullRange(): Range;
}

/** Arguments the keymap passes to a command. */
export interface CommandArgs {
  count?: number;
  register?: string;
}

/** Collects edits for one `editor.edit` call. */
export interface EditorEditBuilder {
  insert(position: Position, text: string): void;
  replace(range: Range, text: string): void;
  delete(range: Range): void;
}

export interface CommandsApi {
  registerCommand(id: string, handler: (args?: CommandArgs) => unknown): Disposable;
  execute(id: string, args?: unknown): Promise<unknown>;
  getCommands(): Promise<string[]>;
}

/** `when` is evaluated against the key context (`normalMode`, `hasSelection`, ...). */
export interface KeyBindingOptions {
  when?: string;
  mode?: Mode;
}

export interface KeymapApi {
  bind(keys: string, target: string, options?: KeyBindingOptions): Disposable;
  unbind(keys: string): void;
  setContext(key: string, value: boolean | string | number): void;
}

export interface ConfigApi {
  get(key: string): Promise<unknown>;
  set(values: Record<string, unknown>): Promise<{ changed: string[] }>;
  set(key: string, value: unknown): Promise<{ changed: string[] }>;
  inspect(key: string): Promise<Inspect>;
  onDidChange(key: string, listener: (event: { key: string }) => void): Disposable;
}

export interface EditorApi {
  activeDocument(): Promise<Document | null>;
  documents(): Promise<Document[]>;
  mode(): Promise<Mode>;
  setMode(mode: Mode): Promise<void>;
  selections(): Promise<Selection[]>;
  setSelections(selections: Selection[]): Promise<void>;
  edit(builder: (builder: EditorEditBuilder) => void): Promise<{ version: number }>;
  undo(): Promise<void>;
  redo(): Promise<void>;
}

export interface WorkspaceApi {
  readonly rootUri: Uri | null;
  readonly name: string | null;
  readonly trusted: boolean;
  folders(): Promise<Uri[]>;
  textDocuments(): Promise<Document[]>;
}

export interface NotificationsApi {
  show(message: string | { level?: "info" | "warn" | "error"; message: string }): Promise<void>;
}

export interface LogChannel {
  info(message: unknown, data?: unknown): Promise<void>;
  warn(message: unknown, data?: unknown): Promise<void>;
  error(message: unknown, data?: unknown): Promise<void>;
}

export interface LogApi {
  createChannel(name: string): LogChannel;
}

export interface RemoteApi {
  readonly isRemote: boolean;
  readonly name: string;
  readonly os: string | null;
  readonly pathSeparator: string;
}

/** Options for creating a statusline item slot. */
export interface StatuslineItemOptions {
  id?: string;
  alignment: "left" | "center" | "right";
  priority?: number;
  text: string;
  color?: string;
  command?: string;
}

/** A controllable statusline slot item. */
export interface StatuslineItem extends Disposable {
  readonly id: string;
  text: string;
  color?: string;
  command?: string;
  show(): void;
  hide(): void;
  dispose(): void;
}

export interface StatuslineApi {
  createItem(options: StatuslineItemOptions): StatuslineItem;
}

export interface UiApi {
  readonly statusline: StatuslineApi;
}

export interface PluginContext {
  readonly pluginId: string;
  readonly version: string;
  readonly capabilities: { has(name: string): boolean };
  readonly log: LogChannel;
}

export const commands: CommandsApi;
export const keymap: KeymapApi;
export const config: ConfigApi;
export const editor: EditorApi;
export const workspace: WorkspaceApi;
export const notifications: NotificationsApi;
export const log: LogApi;
export const remote: RemoteApi;
export const ui: UiApi;
export const context: PluginContext;
