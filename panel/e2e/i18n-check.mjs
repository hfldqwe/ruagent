#!/usr/bin/env node
// i18n dictionary integrity — the check the UI itself cannot fail loudly on.
//
// `t()` in panel/src/i18n.tsx falls back to `dicts.en[key] ?? key`, so a key
// that exists in neither dictionary renders the RAW KEY STRING in the UI and
// nothing anywhere turns red. That is the failure mode this script exists for:
//
//   1. zh and en must have EXACTLY the same key set (a key present in only one
//      dictionary silently renders the other language);
//   2. no duplicate keys inside one dictionary (a duplicate is a silently
//      shadowed string — the later one wins, the earlier one is dead text);
//   3. every literal t("...") reference in the repo must resolve in BOTH
//      dictionaries (fallback makes "only in en" a half-translation, not a bug
//      report).
//
// Non-literal references (t(someVar), t(`a.${b}`)) cannot be resolved by any
// static check. They are listed, not counted as failures — pretending they were
// verified would be the same lie as not checking at all.
//
//   cd panel && node e2e/i18n-check.mjs           # human report, exit 1 on failure
//   cd panel && node e2e/i18n-check.mjs --json    # machine-readable

import { readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, resolve, relative } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const PANEL = resolve(HERE, "..");
const REPO = resolve(PANEL, "..");
const SRC = join(PANEL, "src");

/** Extract the top-level keys of the object literal starting at `start` (index of `{`). */
function objectKeys(text, start) {
  const keys = [];
  let depth = 0;
  let i = start;
  let line = 1 + text.slice(0, start).split("\n").length - 1;
  const lineAt = (idx) => 1 + text.slice(0, idx).split("\n").length - 1;
  while (i < text.length) {
    const ch = text[i];
    if (ch === "/" && text[i + 1] === "/") {
      i = text.indexOf("\n", i);
      if (i < 0) break;
      continue;
    }
    if (ch === "/" && text[i + 1] === "*") {
      i = text.indexOf("*/", i) + 2;
      continue;
    }
    if (ch === '"' || ch === "'" || ch === "`") {
      const quote = ch;
      const from = i;
      i++;
      while (i < text.length) {
        if (text[i] === "\\") i += 2;
        else if (text[i] === quote) break;
        else i++;
      }
      // A string directly inside the top-level object that is followed by `:` is a key.
      if (depth === 1 && quote !== "`") {
        let j = i + 1;
        while (j < text.length && /\s/.test(text[j])) j++;
        if (text[j] === ":") keys.push({ key: text.slice(from + 1, i), line: lineAt(from) });
      }
      i++;
      continue;
    }
    if (ch === "{") depth++;
    else if (ch === "}") {
      depth--;
      if (depth === 0) break;
    }
    i++;
  }
  return keys;
}

function dictKeys(text, name) {
  const m = new RegExp(`const\\s+${name}\\s*:[^=]*=\\s*\\{`).exec(text);
  if (!m) throw new Error(`dictionary "${name}" not found in i18n.tsx`);
  const brace = text.indexOf("{", m.index + m[0].length - 1);
  return objectKeys(text, brace);
}

function walk(dir, out = []) {
  for (const e of readdirSync(dir)) {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx)$/.test(p)) out.push(p);
  }
  return out;
}

const i18nPath = join(SRC, "i18n.tsx");
const src = readFileSync(i18nPath, "utf8");
const zh = dictKeys(src, "zh");
const en = dictKeys(src, "en");
const zhKeys = zh.map((k) => k.key);
const enKeys = en.map((k) => k.key);
const setZh = new Set(zhKeys);
const setEn = new Set(enKeys);

const dup = (arr) => {
  const seen = new Map();
  const d = [];
  for (const k of arr) {
    if (seen.has(k)) d.push(k);
    else seen.set(k, 1);
  }
  return [...new Set(d)];
};
const dupZh = dup(zhKeys);
const dupEn = dup(enKeys);
const onlyZh = zhKeys.filter((k) => !setEn.has(k));
const onlyEn = enKeys.filter((k) => !setZh.has(k));

// ── literal t() references, repo-wide ────────────────────────────────────────
const files = [SRC, join(PANEL, "e2e")].flatMap((d) => walk(d));
const refs = [];
const dynamic = [];
for (const f of files) {
  const text = readFileSync(f, "utf8");
  const rel = relative(REPO, f).split("\\").join("/");
  const lineAt = (idx) => 1 + text.slice(0, idx).split("\n").length - 1;
  // t("key") / t('key') / t("key", {...})
  const re = /(?<![A-Za-z0-9_$.])t\(\s*("([^"\n]*)"|'([^'\n]*)')/g;
  let m;
  while ((m = re.exec(text))) {
    refs.push({ file: rel, line: lineAt(m.index), key: m[2] ?? m[3] });
  }
  // t(var) / t(`...${}`) — unresolvable by design
  const reDyn = /(?<![A-Za-z0-9_$.])t\(\s*([^"'\s)][^)\n]*)/g;
  while ((m = reDyn.exec(text))) {
    dynamic.push({ file: rel, line: lineAt(m.index), expr: m[1].trim().slice(0, 60) });
  }
}
const missingZh = refs.filter((r) => !setZh.has(r.key));
const missingEn = refs.filter((r) => !setEn.has(r.key));

// A key reached through a variable (`t(s.key)`, `t(\`status.${x}\`)`) cannot be
// resolved by the literal scan. The loosest check that is still sound: does the
// key appear as a string literal ANYWHERE outside i18n.tsx? That covers
// `{ key: "home.stat.agents" }` and labelKey tables without pretending to
// understand the call graph. A key that never appears anywhere is a dead-key
// candidate — registered, not a failure.
const outside = files.filter((f) => resolve(f) !== resolve(i18nPath)).map((f) => readFileSync(f, "utf8")).join("\n");
const unreferenced = zhKeys.filter((k) => !outside.includes(`"${k}"`) && !outside.includes(`'${k}'`));
const dynamicFamilies = [...new Set(dynamic.map((d) => /^`([^`$]*)\$\{/.exec(d.expr)?.[1]).filter(Boolean))].map((p) => ({
  prefix: p,
  keys: zhKeys.filter((k) => k.startsWith(p)).length,
}));
const unreferencedNotInFamilies = unreferenced.filter((k) => !dynamicFamilies.some((f) => k.startsWith(f.prefix)));

const report = {
  i18nPath: relative(REPO, i18nPath).split("\\").join("/"),
  counts: { zh: zhKeys.length, en: enKeys.length, refs: refs.length, dynamic: dynamic.length },
  duplicates: { zh: dupZh, en: dupEn },
  keySetDiff: { onlyZh, onlyEn },
  unresolvedRefs: { missingInZh: missingZh, missingInEn: missingEn },
  unreferencedKeys: unreferenced,
  unreferencedNotInFamilies,
  dynamicRefs: dynamic,
  dynamicFamilies,
  pass:
    dupZh.length === 0 &&
    dupEn.length === 0 &&
    onlyZh.length === 0 &&
    onlyEn.length === 0 &&
    missingZh.length === 0 &&
    missingEn.length === 0,
};

if (process.argv.includes("--json")) {
  process.stdout.write(JSON.stringify(report, null, 2) + "\n");
} else {
  console.log(`i18n dictionary integrity — ${report.i18nPath}`);
  console.log(`keys: zh=${zhKeys.length} en=${enKeys.length} · literal t() refs=${refs.length} · dynamic refs=${dynamic.length}`);
  console.log(`duplicates: zh=${dupZh.length ? dupZh.join(",") : "none"} · en=${dupEn.length ? dupEn.join(",") : "none"}`);
  console.log(`key set: only-in-zh=${onlyZh.length ? onlyZh.join(",") : "none"} · only-in-en=${onlyEn.length ? onlyEn.join(",") : "none"}`);
  console.log(`unresolved refs: missing-in-zh=${missingZh.length} missing-in-en=${missingEn.length}`);
  for (const r of [...missingZh, ...missingEn]) console.log(`  ${r.file}:${r.line} t("${r.key}")`);
  if (dynamic.length) {
    console.log(`dynamic refs (not statically resolvable, listed for review): ${dynamic.length}`);
    for (const d of dynamic.slice(0, 12)) console.log(`  ${d.file}:${d.line} t(${d.expr})`);
  }
  if (dynamicFamilies.length) {
    console.log(`dynamic families (t(\`prefix.\${x}\`)): ${dynamicFamilies.map((f) => `${f.prefix}* (${f.keys} keys)`).join(", ")}`);
  }
  console.log(
    `keys with no string literal anywhere outside i18n.tsx: ${unreferenced.length} (of which ${unreferencedNotInFamilies.length} are also outside every dynamic family prefix)`,
  );
  if (unreferencedNotInFamilies.length) console.log(`  dead-key candidates: ${unreferencedNotInFamilies.join(",")}`);
  console.log(report.pass ? "VERDICT: PASS" : "VERDICT: FAIL");
}
process.exit(report.pass ? 0 : 1);
