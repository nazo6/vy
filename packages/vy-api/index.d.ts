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
  lineAt(line: number): Promise<string | null>;
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
  registerCommand(id: string, handler: (args?: unknown) => unknown): Disposable;
  execute<T = unknown>(id: string, args?: unknown): Promise<T>;
  getCommands(): Promise<string[]>;
}

/** `when` is evaluated against the key context (`normalMode`, `hasSelection`, ...). */
export interface KeyBindingOptions {
  when?: string;
  mode?: Mode;
}

export interface KeymapApi {
  bind(keys: string, target: string, options?: KeyBindingOptions): Disposable;
  unbind(params: { keys: string }): Promise<number>;
  setContext(params: { key: string; value: boolean | string | number }): Promise<void>;
}

export interface ConfigApi {
  get(params: { key: string }): Promise<unknown>;
  set(values: Record<string, unknown>): Promise<{ changed: string[] }>;
  set(key: string, value: unknown): Promise<{ changed: string[] }>;
  inspect(params: { key: string }): Promise<Inspect>;
  onDidChange(key: string, listener: (event: { key: string }) => void): Disposable;
}

export interface EditorApi {
  activeDocument(): Promise<Document | null>;
  documents(): Promise<Document[]>;
  mode(): Promise<Mode>;
  setMode(params: { mode: Mode }): Promise<void>;
  selections(): Promise<Selection[]>;
  setSelections(params: { selections: Selection[] }): Promise<void>;
  edit(
    edits: TextEdit[] | ((builder: EditorEditBuilder) => void),
  ): Promise<{ version: number }>;
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

export interface PickerItem {
  id: string;
  title: string;
  description?: string;
  detail?: string;
}

export interface ShowPickerOptions {
  items: PickerItem[];
  placeholder?: string;
}

export interface PickerApi {
  show(options: ShowPickerOptions): Promise<PickerItem | undefined>;
}

export interface StatuslineApi {
  createItem(options: StatuslineItemOptions): StatuslineItem;
}

export interface GutterItemOptions {
  id?: string;
  line: number;
  text: string;
  color?: string;
}

export interface GutterItem extends Disposable {
  readonly id: string;
  dispose(): void;
}

export interface GutterApi {
  createItem(options: GutterItemOptions): GutterItem;
  disposeItem(params: { id: string }): Promise<void>;
  clear(): Promise<void>;
}

export interface HoverOptions {
  line: number;
  character: number;
  text: string;
}

export interface HoverApi {
  show(options: HoverOptions): Promise<void>;
  hide(): Promise<void>;
}

export interface WidgetNode {
  type: string;
  [key: string]: unknown;
}

export interface UiContainerOptions {
  id?: string;
  gap?: "sm" | "md" | "lg";
}

export interface UiTextOptions {
  id?: string;
  text: string;
  color?: string;
  font?: "mono";
}

export interface UiButtonOptions {
  id?: string;
  label: string;
  command: string;
  variant?: "primary" | "danger";
}

export interface UiTextInputOptions {
  id?: string;
  value?: string;
  placeholder?: string;
}

export interface UiApi {
  readonly picker: PickerApi;
  readonly statusline: StatuslineApi;
  readonly gutter: GutterApi;
  readonly hover: HoverApi;
  column(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  row(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  stack(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  text(options: UiTextOptions | string): WidgetNode;
  button(options: UiButtonOptions): WidgetNode;
  textInput(options: UiTextInputOptions): WidgetNode;
  separator(options?: { id?: string }): WidgetNode;
}

export type ViewKind = "editor" | "ui" | "list" | "text" | "image" | "terminal";

export interface ViewOptions {
  kind: ViewKind;
  title?: string;
  singleton?: boolean;
  render?: () => WidgetNode;
}

export interface ViewsApi {
  register(type: string, options: ViewOptions): Disposable;
  get(type: string): ViewOptions | undefined;
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
export const views: ViewsApi;
export const context: PluginContext;
