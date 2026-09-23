// Bundle composition — the "why" behind MASTER §12 row 27 (first-paint JS size).
//
// Row 27 judges the ENTRY CHUNK (dist/assets/index-*.js) against <=350KB /
// gzip <=120KB. A verdict alone does not say what to do about it, and guessing
// ("it's antd") is not evidence. Rollup already knows the answer: every module
// in the chunk carries renderedLength, so this config reports the entry chunk
// grouped by package and prints the biggest modules.
//
// It writes NOTHING into panel/dist: outDir is overridden on the command line.
//
//   cd panel && npx vite build --config e2e/bundle-stats.config.mjs \
//       --outDir <somewhere-outside-the-repo> --emptyOutDir
//
// The same config also reports what a route-level code split would have to move
// (per-view modules), which is what the row's disposition question needs.

import react from "@vitejs/plugin-react";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const PANEL = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const group = (id) => {
  const p = id.split("\\").join("/");
  if (!p.includes("/node_modules/")) return "app src (panel/src)";
  if (/\/node_modules\/(react-dom|scheduler)\//.test(p)) return "react-dom";
  if (/\/node_modules\/react\//.test(p)) return "react";
  if (/\/node_modules\/(antd|@ant-design|rc-[a-z-]+|@rc-component)\//.test(p)) return "antd + rc-*";
  if (/\/node_modules\/(react-markdown|remark[a-z-]*|micromark[a-z-]*|mdast[a-z-]*|unified|hast[a-z-]*|unist[a-z-]*|vfile[a-z-]*|property-information|character-entities[a-z-]*|decode-named-character-reference|html-url-attributes|trim-lines|devlop|zwitch|ccount|markdown-table|longest-streak|bail|trough|is-plain-obj|extend|space-separated-tokens|comma-separated-tokens|parse-entities|stringify-entities)\//.test(p))
    return "markdown stack";
  return "other node_modules";
};

export default {
  root: PANEL,
  plugins: [
    react(),
    {
      name: "bundle-composition",
      generateBundle(_options, bundle) {
        const out = [];
        for (const [file, item] of Object.entries(bundle)) {
          if (item.type !== "chunk") continue;
          const per = {};
          const mods = [];
          for (const [id, m] of Object.entries(item.modules)) {
            const g = group(id);
            per[g] = (per[g] ?? 0) + (m.renderedLength ?? 0);
            mods.push({ id: id.split("\\").join("/"), len: m.renderedLength ?? 0, group: g });
          }
          out.push({ file, bytes: item.code.length, isEntry: item.isEntry, groups: per, modules: mods });
        }
        this.emitFile({
          type: "asset",
          fileName: "bundle-composition.json",
          source: JSON.stringify({ generatedAt: new Date().toISOString(), chunks: out }, null, 2),
        });
      },
    },
  ],
  define: { __BUILD_ID__: JSON.stringify("bundle-stats") },
  build: { outDir: "dist" }, // overridden by --outDir on the command line
};
