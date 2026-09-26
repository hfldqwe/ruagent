#!/usr/bin/env node
// Post-switch evidence check (t307).
//
// WHY THIS EXISTS: a daemon switch is verified today by the binary's md5, the
// schema version and a few endpoint reads. Those prove the BINARY is new. They
// do NOT prove the third level of the ladder (construction -> sent -> received):
// that an agent actually RECEIVED what the new code injects. After the t279
// window that gap was measured, not assumed: 146 transcripts, 0 modified in the
// hour after the switch, 0 context_injected events -- the canary runs in a
// temp root and leaves the live store untouched.
//
// WHAT IT READS: <root>/data/transcripts/*.jsonl, the daemon's own first-class
// events. Nothing is written, no endpoint is called, and /api/v1/recall is
// deliberately NOT used -- it would append a recall_log row and make the check a
// producer of the data it inspects.
//
// THE ONE RULE THAT MATTERS (7.25): a run that finds nothing must NOT exit 0.
// "0 events" reported as success is the same mistake as a skipped test being
// read as green. No evidence is a FAILURE here, and the failure names which
// assertion was missing.
//
// WHAT IT DOES NOT COVER (read this before trusting a green):
//   - it only sees instances that write to THIS transcripts directory; a temp
//     root (the canary, the e2e tests) leaves no trace here at all;
//   - it cannot prove any OTHER instance received anything, and it says nothing
//     about whether the agent understood or used the block -- only that the
//     text was in the render the daemon sent;
//   - it checks the RENDER, so a truncation between render and model would not
//     be caught by it (the echo is what proves delivery, and an echo is the
//     mock agent's property, not the real one's);
//   - mtime is the file's, not the event's: a transcript touched later for any
//     reason counts as "after".
//
// USAGE
//   node scripts/ruagent-post-switch-check.mjs --since <iso|epochSeconds>
//        [--marker <string>] [--require-wiki]
//        [--transcripts <dir>] [--root <dir>] [--json]
//
// EXIT CODES: 0 = evidence found and every required assertion passed;
//             1 = no evidence, or an assertion failed (usable as a gate);
//             2 = usage error.

import fs from "node:fs";

// Built from char codes so this file survives being copied through any
// transport that eats backslash escapes.
const NL = String.fromCharCode(10);
const CR = String.fromCharCode(13);
import os from "node:os";
import path from "node:path";

function parseArgs(argv) {
  const out = { since: null, marker: null, requireWiki: false, transcripts: null, root: null, json: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--since") out.since = argv[++i];
    else if (a === "--marker") out.marker = argv[++i];
    else if (a === "--require-wiki") out.requireWiki = true;
    else if (a === "--transcripts") out.transcripts = argv[++i];
    else if (a === "--root") out.root = argv[++i];
    else if (a === "--json") out.json = true;
    else if (a === "--help" || a === "-h") out.help = true;
    else return { error: "unknown argument: " + a };
  }
  return out;
}

function parseSince(raw) {
  if (!raw) return null;
  const asNum = Number(raw);
  if (Number.isFinite(asNum) && String(asNum) === raw.trim()) {
    // Epoch seconds (the daemon's own switch logs use seconds) or ms.
    return asNum > 1e11 ? asNum : asNum * 1000;
  }
  const t = Date.parse(raw);
  return Number.isFinite(t) ? t : null;
}

function transcriptsDir(args) {
  if (args.transcripts) return args.transcripts;
  const root = args.root || path.join(os.homedir(), ".ruagent");
  return path.join(root, "data", "transcripts");
}

function eventsIn(file) {
  let text;
  try {
    text = fs.readFileSync(file, "utf8");
  } catch {
    return [];
  }
  const out = [];
  for (const raw of text.split(NL)) {
    const line = raw.endsWith(CR) ? raw.slice(0, -1) : raw;
    if (!line.trim()) continue;
    let v;
    try {
      v = JSON.parse(line);
    } catch {
      continue;
    }
    if (v && v.event && v.event.type === "context_injected") out.push(v.event);
  }
  return out;
}

const args = parseArgs(process.argv.slice(2));
if (args.error || args.help) {
  if (args.error) {
    console.log("usage error: " + args.error);
  } else {
    console.log("usage: node scripts/ruagent-post-switch-check.mjs --since <iso|epochSeconds> [--marker <string>] [--require-wiki] [--transcripts <dir>] [--root <dir>] [--json]");
    console.log("");
    console.log("WHAT IT DOES NOT COVER (read before trusting a green):");
    console.log("  - only instances that write to THIS transcripts directory; a temp root (canary, e2e tests) leaves no trace here;");
    console.log("  - it cannot prove any OTHER instance received anything, and says nothing about whether the agent used the block;");
    console.log("  - it checks the RENDER, so a truncation between render and model is invisible to it (the echo proves delivery, and the echo is the mock agent's property);");
    console.log("  - mtime is the file's, not the event's: any later touch counts as after.");
  }
  process.exit(args.error ? 2 : 0);
}
const sinceMs = parseSince(args.since);
if (sinceMs === null) {
  console.log("usage error: --since needs an ISO timestamp or epoch seconds (got: " + args.since + ")");
  process.exit(2);
}

const dir = transcriptsDir(args);
let files = [];
try {
  files = fs.readdirSync(dir).filter((f) => f.endsWith(".jsonl"));
} catch (e) {
  console.log("transcripts_after_since=0 (directory unreadable: " + dir + ": " + e.message + ")");
  console.log("context_injected_events=0");
  console.log("render_assertions=NOT_CHECKED");
  console.log("FAIL no evidence: cannot read the transcripts directory " + dir);
  process.exit(1);
}

const after = files
  .map((f) => {
    const full = path.join(dir, f);
    return { full, mtime: fs.statSync(full).mtimeMs };
  })
  .filter((x) => x.mtime > sinceMs);

let events = 0;
let withKnowledge = 0;
let withWiki = 0;
let withMarker = 0;
const samples = [];
for (const f of after) {
  const evs = eventsIn(f.full);
  for (const ev of evs) {
    events += 1;
    const r = typeof ev.render === "string" ? ev.render : "";
    if (r.includes("<knowledge>")) withKnowledge += 1;
    if (r.includes("<wiki>")) withWiki += 1;
    if (args.marker && r.includes(args.marker)) withMarker += 1;
    if (samples.length < 3) samples.push({ file: path.basename(f.full), render: r });
  }
}

const sinceIso = new Date(sinceMs).toISOString();
console.log("transcripts_after_since=" + after.length + " (dir=" + dir + ", since=" + sinceIso + ", scanned=" + files.length + ")");
console.log("context_injected_events=" + events);
console.log(
  "render_assertions=knowledge:" + withKnowledge + " wiki:" + withWiki +
    (args.marker ? " marker(" + args.marker + "):" + withMarker : " marker:none-requested") +
    " of " + events + " events"
);

if (args.json) {
  console.log(JSON.stringify({ dir, since: sinceIso, filesAfter: after.length, events, withKnowledge, withWiki, marker: args.marker, withMarker, samples }, null, 2));
}

// The failure side. Every branch names WHICH assertion is missing.
const missing = [];
if (events === 0) {
  missing.push("no events (nothing was modified after " + sinceIso + " that carries a context_injected event)");
  console.log("NO EVIDENCE: no context_injected event after the switch window in this instance.");
  console.log("  A temp-root instance (canary, e2e tests) leaves nothing here by design -- drive one real chat or run against the live daemon first, then re-run this check.");
} else {
  if (withKnowledge === 0) missing.push("no knowledge block in any of the " + events + " events");
  if (args.requireWiki && withWiki === 0) missing.push("no wiki block in any of the " + events + " events");
  if (args.marker && withMarker === 0) missing.push("marker string not found: " + args.marker);
}
if (missing.length > 0) {
  console.log("FAIL " + missing.join("; "));
  process.exit(1);
}
console.log("PASS evidence found: " + events + " context_injected event(s) after the switch, knowledge present" + (args.requireWiki ? ", wiki present" : "") + (args.marker ? ", marker present" : "") + ".");
process.exit(0);
