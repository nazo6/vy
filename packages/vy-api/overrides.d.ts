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

/** Something that can be released when a script reloads. */
export interface Disposable {
  dispose(): void;
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

/** Collects edits for one `editor.edit` call. */
export interface EditorEditBuilder {
  insert(position: Position, text: string): void;
  replace(range: Range, text: string): void;
  delete(range: Range): void;
}

export interface CommandsSugar {
  registerCommand(
    id: CommandId | (string & {}),
    handler: (args?: unknown) => unknown,
  ): Disposable;
  /** Typed overload for commands declared through `CommandMap` augmentation. */
  execute<K extends keyof CommandMap>(id: K, args: CommandMap[K]): Promise<unknown>;
  execute(id: CommandId | (string & {}), args?: unknown): Promise<unknown>;
}

export interface KeymapSugar {
  bind(
    keys: string,
    target: CommandId | (string & {}),
    options?: { when?: string; mode?: Mode },
  ): Disposable;
}

export interface ConfigSugar {
  set(values: Record<string, unknown>): Promise<{ changed: string[] }>;
  set(key: SettingsKey | (string & {}), value: unknown): Promise<{ changed: string[] }>;
  get<K extends SettingsKey>(params: { key: K }): Promise<SettingsMap[K]>;
  get(params: { key: string }): Promise<unknown>;
  onDidChange(key: string, listener: (event: { key: string }) => void): Disposable;
}

export interface EditorSugar {
  activeDocument(): Promise<Document | null>;
  documents(): Promise<Document[]>;
  edit(
    edits: TextEdit[] | ((builder: EditorEditBuilder) => void),
  ): Promise<{ version: number }>;
}

export interface WorkspaceSugar {
  readonly rootUri: Uri | null;
  readonly name: string | null;
  readonly trusted: boolean;
  folders(): Uri[];
  textDocuments(): Promise<Document[]>;
}

export interface NotificationsSugar {
  show(message: string): Promise<void>;
}

export interface LogChannel {
  info(message: unknown, data?: unknown): Promise<void>;
  warn(message: unknown, data?: unknown): Promise<void>;
  error(message: unknown, data?: unknown): Promise<void>;
}

export interface LogSugar {
  createChannel(name: string): LogChannel;
}

/** Options for `ui.statusline.createItem`; the shim generates the id. */
export type StatuslineItemOptions = Omit<StatuslineCreateParams, "id"> & { id?: string };

export interface StatuslineItem extends Disposable {
  readonly id: string;
  text: string;
  color?: ThemeColor;
  command?: string;
  show(): void;
  hide(): void;
}

export interface UiStatuslineSugar {
  createItem(options: StatuslineItemOptions): StatuslineItem;
}

/** Options for `ui.gutter.createItem`; the shim generates the id. */
export type GutterItemOptions = Omit<GutterCreateParams, "id"> & { id?: string };

export interface GutterItem extends Disposable {
  readonly id: string;
}

export interface UiGutterSugar {
  createItem(options: GutterItemOptions): GutterItem;
}

/** Options for `ui.picker.show`; the shim generates the request id. */
export type ShowPickerOptions = Omit<PickerShowParams, "id">;

export interface UiPickerSugar {
  show(options: ShowPickerOptions): Promise<PickerItem | undefined>;
}

export interface UiContainerOptions {
  id?: string;
  gap?: Gap;
}

export interface UiTextOptions {
  id?: string;
  text: string;
  color?: ThemeColor;
  font?: Font;
}

export interface UiButtonOptions {
  id?: string;
  label: string;
  command: string;
  variant?: ButtonVariant;
}

export interface UiTextInputOptions {
  id?: string;
  value?: string;
  placeholder?: string;
}

export interface UiSugar {
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

export interface ViewsSugar {
  register(type: string, options: ViewOptions): Disposable;
  get(type: string): ViewOptions | undefined;
}

export interface RemoteApi {
  readonly isRemote: boolean;
  readonly name: string;
  readonly os: string | null;
  readonly pathSeparator: string;
}

export interface PluginContext {
  readonly pluginId: string;
  readonly version: string;
  readonly capabilities: { has(name: string): boolean };
  readonly log: LogChannel;
}

export const remote: RemoteApi;
export const context: PluginContext;
