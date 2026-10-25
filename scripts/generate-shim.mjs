import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const emitted = "target/api-shim/crates/vy-js/src";
const header =
  "// This file was generated from crates/vy-js/src/shim.ts by tsc.\n" +
  "// Do not edit it manually. Regenerate with `pnpm generate:api`.\n\n";
const declaration =
  header +
  readFileSync(`${emitted}/shim.d.ts`, "utf8").replaceAll(
    "../../../packages/vy-api/",
    "./",
  );

mkdirSync("crates/vy-js/src/generated", { recursive: true });
copyFileSync(`${emitted}/shim.js`, "crates/vy-js/src/generated/shim.js");
mkdirSync(dirname("packages/vy-api/shim.d.ts"), { recursive: true });
writeFileSync("packages/vy-api/shim.d.ts", declaration);
