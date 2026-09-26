#!/usr/bin/env node
// t319: `vite build` empties its outDir BEFORE it writes the new bundle, so a
// rebuild made the daemon answer 404 on / for a couple of seconds (measured: a
// tight curl loop during a build saw non-200). Everyone who ran `npm run build`
// as a verify step opened that window for whoever was using the panel.
//
// This wrapper keeps the served directory usable for the WHOLE build:
//   1. everything is built into dist-staging -- dist is not touched at all;
//   2. the new assets are copied into dist/assets (files are ADDED; the old
//      ones stay because the old index.html still points at them);
//   3. dist/index.html is replaced by writing index.html.tmp and RENAMING it
//      over the old one (a rename is atomic on Windows and on POSIX);
//   4. only then are the assets the new index no longer references removed.
// A failure anywhere before step 3 leaves dist exactly as it was: the served
// panel is never a half-written bundle.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const panel = dirname(dirname(fileURLToPath(import.meta.url)));
const dist = join(panel, "dist");
const staging = join(panel, "dist-staging");

const run = (cmd) => {
  const r = spawnSync(cmd, { cwd: panel, stdio: "inherit", shell: true });
  if (r.status !== 0) {
    console.error(`build-panel: \`${cmd}\` exited ${r.status}; dist untouched`);
    process.exit(r.status ?? 1);
  }
};

run("node e2e/i18n-check.mjs");
run("npx tsc -b");
run("npx tsc -p e2e/tsconfig.json --noEmit");
// Deliberate failure hook: the acceptance asks for a reading where a FAILED
// build leaves dist as the old usable version. This fails after the type
// checks and before vite, i.e. before anything writes to dist.
if (process.env.PANEL_BUILD_FORCE_FAIL === "1") {
  console.error("build-panel: PANEL_BUILD_FORCE_FAIL=1 -- failing before vite; dist untouched");
  process.exit(1);
}
rmSync(staging, { recursive: true, force: true });
run("npx vite build --outDir dist-staging --emptyOutDir");

// --- the only part that touches dist -------------------------------------
const stagingAssets = join(staging, "assets");
const distAssets = join(dist, "assets");
mkdirSync(distAssets, { recursive: true });
const newAssets = existsSync(stagingAssets) ? readdirSync(stagingAssets) : [];
for (const f of newAssets) copyFileSync(join(stagingAssets, f), join(distAssets, f));
// Write-then-rename: the served index.html is never a truncated file.
const tmp = join(dist, "index.html.tmp");
writeFileSync(tmp, readFileSync(join(staging, "index.html")));
renameSync(tmp, join(dist, "index.html"));
// Housekeeping only: after the swap the old names are unreferenced.
for (const f of readdirSync(distAssets)) if (!newAssets.includes(f)) rmSync(join(distAssets, f), { force: true });
rmSync(staging, { recursive: true, force: true });
console.log(`build-panel: dist updated (${newAssets.length} assets, index.html swapped by rename)`);