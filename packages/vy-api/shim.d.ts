// This file was generated from crates/vy-js/src/shim.ts by tsc.
// Do not edit it manually. Regenerate with `pnpm generate:api`.

import type { CommandsApi, ConfigApi, ContextApi, Disposable, DocumentApi, EditorApi, FsApi, KeymapApi, LogApi, NotificationsApi, ProcessApi, UiApi, UiGutterApi, UiHoverApi, UiPickerApi, UiStatuslineApi, ViewsApi, WorkspaceApi } from "./api-base";
import type { BuiltinCommandId, BuiltinCommandMap } from "./types";
declare global {
    namespace Vy {
        /** Augment this interface to type the arguments of your own commands. */
        interface CommandMap {
        }
    }
}
type RegisterHandler<K extends string> = K extends keyof BuiltinCommandMap ? never : K extends keyof Vy.CommandMap ? (args: Vy.CommandMap[K]) => unknown : (args?: unknown) => unknown;
declare function registerCommand<const K extends string>(id: K, handler: RegisterHandler<K>): Disposable;
type ExecuteParams<K extends string> = K extends keyof BuiltinCommandMap ? {
    id: K;
    args?: BuiltinCommandMap[K];
} : K extends keyof Vy.CommandMap ? undefined extends Vy.CommandMap[K] ? {
    id: K;
    args?: Vy.CommandMap[K];
} : {
    id: K;
    args: Vy.CommandMap[K];
} : {
    id: K;
    args?: unknown;
};
declare function execute<const K extends string>(params: ExecuteParams<K>): Promise<unknown>;
export declare const commands: CommandsApi & {
    registerCommand: typeof registerCommand;
    execute: typeof execute;
};
export declare const config: ConfigApi;
export declare const context: ContextApi;
export declare const document: DocumentApi;
export declare const editor: EditorApi;
export declare const fs: FsApi;
declare function bind<K extends string & (BuiltinCommandId | keyof Vy.CommandMap)>(params: {
    keys: string;
    command: K;
    when?: string | null;
}): Promise<void>;
declare function bind(params: {
    keys: string;
    command: string;
    when?: string | null;
}): Promise<void>;
export declare const keymap: KeymapApi & {
    bind: typeof bind;
};
export declare const log: LogApi;
export declare const notifications: NotificationsApi;
export declare const process: ProcessApi;
export declare const views: ViewsApi;
export declare const workspace: WorkspaceApi;
export declare const ui: UiApi & {
    gutter: UiGutterApi;
    hover: UiHoverApi;
    picker: UiPickerApi;
    statusline: UiStatuslineApi;
};
export {};
