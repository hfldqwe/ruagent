import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Build stamp: shown in the panel footer so "which build am I on" is
// answerable at a glance (stale-cache confusion, 2026-09-13).
const BUILD_ID = new Date().toISOString().slice(5, 16).replace("T", " ");

// ── Vendor split (MASTER §12 row 27) ──────────────────────────────────────
// The audit reads the built chunks, so the names are part of the contract:
//   vendor    the framework floor: react + react-dom + scheduler + antd +
//             rc-* + @ant-design + everything else from node_modules
//             (69.7% of the old 1406KB bundle)
//   markdown  unified + micromark + mdast + remark (16.3%), reached only
//             through dynamic imports
// Application code returns undefined on purpose: Rollup then keeps one
// chunk per lazily-imported route (App.tsx).
//
// The markdown stack is reached only through dynamic imports
// (src/views/markdown.tsx and the lazy Wiki route), so this chunk must not
// appear in the entry's static import list — that is row 27's structural
// assertion.
const MARKDOWN_RE =
  /[\\/]node_modules[\\/](react-markdown|remark-|micromark|mdast-|unified|hast-|unist-|vfile|property-information|space-separated-tokens|comma-separated-tokens|character-entities|decode-named-character-reference|trim-lines|devlop|html-url-attributes|style-to-|zwitch|longest-streak|ccount|markdown-table|escape-string-regexp|bail|trough|is-plain-obj|extend|web-namespaces|parse-entities|is-decimal|is-hexadecimal|is-alphanumerical|is-alphabetical|stringify-entities)/;
function manualChunks(id: string): string | undefined {
  if (!id.includes("node_modules")) return undefined;
  if (MARKDOWN_RE.test(id)) return "markdown";
  // ONE vendor chunk for the whole framework (react + react-dom + antd + rc-*
  // + the rest). Row 27 reads the artifact by NAME: vendor-* is the framework
  // floor and markdown-* the lazily loaded stack; everything else counts as
  // the app layer. Splitting react/antd into their own names would push the
  // 980KB floor into the app budget.
  return "vendor";
}

export default defineConfig({
  plugins: [react()],
  define: { __BUILD_ID__: JSON.stringify(BUILD_ID) },
  build: {
    outDir: "dist",
    rollupOptions: { output: { manualChunks } },
  },
  server: {
    // Dev against a running daemon; the daemon serves /api.
    proxy: { "/api": "http://127.0.0.1:8787" },
  },
});
