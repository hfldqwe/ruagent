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
