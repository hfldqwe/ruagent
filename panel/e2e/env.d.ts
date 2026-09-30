// The e2e specs run in Node, but this project deliberately has no @types/node
// dependency, so the one global they use is declared here. Keeping it minimal is
// the point: this file exists so that panel/e2e/ can be TYPE-CHECKED at all.
//
// Why it matters (t190): t185 refactored registry.spec.ts to wrap its body in
// try/finally and moved the body into a helper without passing page through. The
// result was ReferenceError: page is not defined -- and npx tsc -b --noEmit did
// not catch it, because tsconfig.json only included [src, vite.config.ts]. The
// bug was found by RUNNING the suite. This file plus tsconfig.e2e.json close that
// gap: the e2e directory is now checked, so that class of mistake is caught
// before anything runs.
declare const process: { env: Record<string, string | undefined> };

// The one Node module an e2e spec needs (settings-capabilities.spec.ts reads the
// daemon's OWN policy.toml back off disk: `options_set` says the API thinks the
// key is there, and the file is what "saved" means for a config). Declared
// minimally for the same reason as `process` above -- without @types/node, an
// `import ... from "node:fs"` is a TS2307 and this directory is type-checked by
// `npm run build` (tsc -p e2e/tsconfig.json --noEmit). Widen only by adding the
// signature a spec actually calls.
declare module "node:fs" {
  export function readFileSync(path: string, encoding: "utf8"): string;
}
