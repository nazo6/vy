// TypeDoc entry point for the `vy-api` reference.
//
// This file is not published and is not imported by scripts. It re-exports the
// hand-written sugar interfaces that `index.d.ts` only references, so TypeDoc
// can document their methods alongside the generated declarations. Keep the
// sugar list in sync with `SUGAR` in `crates/vy-api-schema/src/codegen.rs`.

export * from "./index";

export type {
  CommandsSugar,
  ConfigSugar,
  EditorSugar,
  KeymapSugar,
  LogSugar,
  NotificationsSugar,
  UiGutterSugar,
  UiPickerSugar,
  UiStatuslineSugar,
  UiSugar,
  ViewsSugar,
  WorkspaceSugar,
} from "./overrides";
