// Hand-written declarations for `vy-api`.
//
// Everything else in `index.d.ts` is generated from `HOST_METHODS` in
// `crates/vy-api-schema`. This file only holds the wrappers that keep state in
// the shim: command handlers, config listeners, the edit builder, `Document`,
// slot handles, the picker, views, and the widget builders.

import type {
  ButtonVariant,
  CommandId,
  Font,
  Gap,
  GutterCreateParams,
  Mode,
  PickerItem,
  PickerShowParams,
  Position,
  Range,
  SettingsKey,
  SettingsMap,
  StatuslineCreateParams,
  TextEdit,
  ThemeColor,
  Uri,
  WidgetNode,
} from "../vy-api-types";
import type { CommandMap } from "./index";

/** A registration or handle that can be released when a script reloads. */
export interface Disposable {
  /** Releases the resource. Calling this more than once is safe. */
  dispose(): void;
}

/** The open document. Text and positions are in UTF-16 code units. */
export interface Document {
  /** Document id assigned by the host. */
  readonly id: number;
  /** Document URI. */
  readonly uri: Uri;
  /** Language identifier reported by the host. */
  readonly languageId: string;
  /** Version that increases after each applied edit. */
  readonly version: number;
  /** Number of lines in the document. */
  readonly lineCount: number;
  /** Position just after the last character. */
  readonly end: Position;
  /** Returns the text, optionally limited to `range`. */
  getText(range?: Range): Promise<string>;
  /** Returns the text of `line`, or null when the line is out of range. */
  lineAt(line: number): Promise<string | null>;
  /** Converts a UTF-16 offset to a position. */
  positionAt(offset: number): Promise<Position>;
  /** Converts a position to a UTF-16 offset. */
  offsetAt(position: Position): Promise<number>;
  /** Returns the range that covers the whole document. */
  fullRange(): Range;
}

/** Collects edits for one `editor.edit` call. Edits are applied in call order. */
export interface EditorEditBuilder {
  /** Inserts `text` at `position`. */
  insert(position: Position, text: string): void;
  /** Replaces `range` with `text`. */
  replace(range: Range, text: string): void;
  /** Deletes `range`. */
  delete(range: Range): void;
}

/** Registers command handlers and runs commands by id. */
export interface CommandsSugar {
  /**
   * Registers a handler for a command id.
   *
   * @example
   * ```ts
   * const registration = commands.registerCommand("my.greet", () => {
   *   notifications.show("Hello");
   * });
   * ```
   */
  registerCommand(
    id: CommandId | (string & {}),
    handler: (args?: unknown) => unknown,
  ): Disposable;
  /**
   * Typed overload for commands declared through `CommandMap` augmentation.
   *
   * @example
   * ```ts
   * declare module "vy-api" {
   *   interface CommandMap {
   *     "my.greet": { name: string };
   *   }
   * }
   *
   * await commands.execute("my.greet", { name: "Ada" });
   * ```
   */
  execute<K extends keyof CommandMap>(id: K, args: CommandMap[K]): Promise<unknown>;
  /** Runs a command by id with an optional argument object. */
  execute(id: CommandId | (string & {}), args?: unknown): Promise<unknown>;
}

/** Binds keys and updates keymap context values. */
export interface KeymapSugar {
  /**
   * Binds a key sequence to a command.
   *
   * @example
   * ```ts
   * keymap.bind("Ctrl-s", "vy.document.save", { when: "editorFocus" });
   * ```
   */
  bind(
    keys: string,
    target: CommandId | (string & {}),
    options?: { when?: string; mode?: Mode },
  ): Disposable;
}

/** Reads and writes settings. */
export interface ConfigSugar {
  /** Sets several setting values at once. */
  set(values: Partial<SettingsMap> & Record<string, unknown>): Promise<{ changed: string[] }>;
  /** Sets one setting value with a typed key when known. */
  set<K extends string>(
    key: K,
    value: K extends SettingsKey ? SettingsMap[K] : unknown,
  ): Promise<{ changed: string[] }>;
  /**
   * Returns the typed value of a setting key.
   *
   * @example
   * ```ts
   * const fontSize = await config.get({ key: "ui.fontSize" });
   * ```
   */
  get<K extends SettingsKey>(params: { key: K }): Promise<SettingsMap[K]>;
  /** Returns the value of a setting key without a typed key. */
  get(params: { key: string }): Promise<unknown>;
  /** Calls `listener` when the effective value of `key` changes. */
  onDidChange(key: string, listener: (event: { key: string }) => void): Disposable;
}

/** Reads and edits the active document. */
export interface EditorSugar {
  /** Returns the document in the focused pane, or null when none is open. */
  activeDocument(): Promise<Document | null>;
  /** Returns every open document. */
  documents(): Promise<Document[]>;
  /**
   * Applies edits in one transaction. `edits` can be an array of `TextEdit`
   * values or a callback that collects edits on an `EditorEditBuilder`.
   *
   * @example
   * ```ts
   * await editor.edit((builder) => {
   *   builder.insert({ line: 0, character: 0 }, "hello");
   * });
   * ```
   */
  edit(
    edits: TextEdit[] | ((builder: EditorEditBuilder) => void),
  ): Promise<{ version: number }>;
  /** Calls `listener` when the active document/editor changes. */
  onDidChangeActiveDocument(listener: (document: Document | null) => void): Disposable;
  /** Calls `listener` when the active document/editor changes (alias). */
  onDidChangeActiveEditor(listener: (document: Document | null) => void): Disposable;
  /** Calls `listener` when selections in the active editor change. */
  onDidChangeSelection(listener: (event: { selections: Selection[] }) => void): Disposable;
}

/** Describes the workspace and its open documents. */
export interface WorkspaceSugar {
  /** URI of the workspace root, or null when no folder is open. */
  readonly rootUri: Uri | null;
  /** Display name of the workspace, or null when no folder is open. */
  readonly name: string | null;
  /** Whether the workspace is trusted. */
  readonly trusted: boolean;
  /** Returns the workspace folders. */
  folders(): Uri[];
  /** Returns the open text documents. */
  textDocuments(): Promise<Document[]>;
  /** Calls `listener` when workspace folders change. */
  onDidChangeWorkspaceFolders(listener: (event: { folders: string[] }) => void): Disposable;
  /** Calls `listener` when workspace folders change (alias). */
  onDidChangeFolders(listener: (event: { folders: string[] }) => void): Disposable;
}

/** Shows notifications to the user. */
export interface NotificationsSugar {
  /** Shows `message` as a notification. */
  show(message: string): Promise<void>;
}

/** A named log channel backed by the host's log file. */
export interface LogChannel {
  /** Writes an informational entry. */
  info(message: unknown, data?: unknown): Promise<void>;
  /** Writes a warning entry. */
  warn(message: unknown, data?: unknown): Promise<void>;
  /** Writes an error entry. */
  error(message: unknown, data?: unknown): Promise<void>;
}

/** Writes to plugin log channels. */
export interface LogSugar {
  /** Creates a log channel. The name is included in every entry. */
  createChannel(name: string): LogChannel;
}

/** Options for `ui.statusline.createItem`; the shim generates the id. */
export type StatuslineItemOptions = Omit<StatuslineCreateParams, "id"> & { id?: string };

/** A status line item. Update its fields, then call `show` to redraw it. */
export interface StatuslineItem extends Disposable {
  /** Item id assigned by the shim. */
  readonly id: string;
  /** Text shown in the status line. */
  text: string;
  /** Optional theme color. */
  color?: ThemeColor;
  /** Optional command id run when the item is clicked. */
  command?: string;
  /** Shows or updates the item. */
  show(): void;
  /** Hides the item without disposing it. */
  hide(): void;
}

/** Creates and updates status line items. */
export interface UiStatuslineSugar {
  /**
   * Creates a status line item.
   *
   * @example
   * ```ts
   * const item = ui.statusline.createItem({ alignment: "right", text: "ready" });
   * item.text = "busy";
   * item.show();
   * ```
   */
  createItem(options: StatuslineItemOptions): StatuslineItem;
}

/** Options for `ui.gutter.createItem`; the shim generates the id. */
export type GutterItemOptions = Omit<GutterCreateParams, "id"> & { id?: string };

/** A gutter item shown next to a line. */
export interface GutterItem extends Disposable {
  /** Item id assigned by the shim. */
  readonly id: string;
}

/** Creates and removes gutter items. */
export interface UiGutterSugar {
  /** Creates a gutter item. */
  createItem(options: GutterItemOptions): GutterItem;
}

/** Options for `ui.picker.show`; the shim generates the request id. */
export type ShowPickerOptions = Omit<PickerShowParams, "id">;

/** Shows a picker and returns the chosen item. */
export interface UiPickerSugar {
  /**
   * Shows a picker. The promise resolves with the chosen item, or undefined
   * when the picker is dismissed.
   */
  show(options: ShowPickerOptions): Promise<PickerItem | undefined>;
}

/** Options shared by the container builders. */
export interface UiContainerOptions {
  /** Element id used by the host for updates. */
  id?: string;
  /** Space between children. */
  gap?: Gap;
}

/** Options for `ui.text`. */
export interface UiTextOptions {
  /** Element id used by the host for updates. */
  id?: string;
  /** Text to display. */
  text: string;
  /** Optional theme color. */
  color?: ThemeColor;
  /** Optional font. */
  font?: Font;
}

/** Options for `ui.button`. */
export interface UiButtonOptions {
  /** Element id used by the host for updates. */
  id?: string;
  /** Button label. */
  label: string;
  /** Command id run when the button is pressed. */
  command: string;
  /** Optional visual variant. */
  variant?: ButtonVariant;
}

/** Options for `ui.textInput`. */
export interface UiTextInputOptions {
  /** Element id used by the host for updates. */
  id?: string;
  /** Initial value. */
  value?: string;
  /** Placeholder shown while the input is empty. */
  placeholder?: string;
}

/** Builds the declarative widget tree rendered by the host. */
export interface UiSugar {
  /** Builds a vertical container. */
  column(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  /** Builds a horizontal container. */
  row(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  /** Builds a container that stacks children vertically without gaps. */
  stack(options: UiContainerOptions | WidgetNode[], children?: WidgetNode[]): WidgetNode;
  /** Builds a text node. */
  text(options: UiTextOptions | string): WidgetNode;
  /** Builds a button that runs a command. */
  button(options: UiButtonOptions): WidgetNode;
  /** Builds a text input. */
  textInput(options: UiTextInputOptions): WidgetNode;
  /** Builds a separator. */
  separator(options?: { id?: string }): WidgetNode;
}

/** Kinds of pane views the host can render. */
export type ViewKind = "editor" | "ui" | "list" | "text" | "image" | "terminal";

/** Options for `views.register`. */
export interface ViewOptions {
  /** Kind of view to create. */
  kind: ViewKind;
  /** Title shown in the pane header. */
  title?: string;
  /** Whether only one instance of this view can be open. */
  singleton?: boolean;
  /** Returns the widget tree for a `ui` view. */
  render?: () => WidgetNode;
}

/** Registers and looks up pane views. */
export interface ViewsSugar {
  /**
   * Registers a view type.
   *
   * @example
   * ```ts
   * views.register("my.status", {
   *   kind: "ui",
   *   title: "Status",
   *   render: () => ui.text("ready"),
   * });
   * ```
   */
  register(type: string, options: ViewOptions): Disposable;
  /** Returns the registration for a view type, or undefined. */
  get(type: string): ViewOptions | undefined;
}

/** Information about the remote connection. */
export interface RemoteApi {
  /** Whether the host is running over a remote connection. */
  readonly isRemote: boolean;
  /** Connection name, or an empty string for a local session. */
  readonly name: string;
  /** Operating system reported by the remote host, or null locally. */
  readonly os: string | null;
  /** Path separator used by the remote host. */
  readonly pathSeparator: string;
}

/** Information about the running plugin. */
export interface PluginContext {
  /** Plugin id from the manifest. */
  readonly pluginId: string;
  /** Plugin version from the manifest. */
  readonly version: string;
  /** Capabilities granted to the plugin. */
  readonly capabilities: { has(name: string): boolean };
  /** Default log channel for this plugin. */
  readonly log: LogChannel;
}

/** Information about the remote connection. */
export const remote: RemoteApi;

/** Information about the running plugin. */
export const context: PluginContext;
