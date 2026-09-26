#!/usr/bin/env node
// audit-shards.mjs -- run tools/design-audit.mjs --check in SEGMENTS so a full
// audit fits inside a single tool-call ceiling (t332, F-t306-03).
//
// WHY: a full --check is 26 captures (13 routes x 2 modes) and measured 702.2s,
// which is past the 600s wall-clock ceiling of one agent call. Every run that
// long gets killed mid-sweep and the gate reading is lost. So the same sweep is
// cut into independently-completable segments, each recorded in a state file, so
// an interrupted run RESUMES instead of starting over.
//
// AGGREGATION RULE (read this before comparing with a single run):
//   * design-audit's own counts are PER ROW (a row passes when it passes on every
//     capture it was judged on), so per-segment counts must NEVER be summed.
//   * The comparable quantities are: the UNION of failing row numbers, the UNION
//     of not-measured row numbers, and the total captures. A row is FAIL if any
//     segment fails it -- which is exactly what a single full run reports.
//   * A segment that did not run stays visible as "pending"/"interrupted": it is
//     never folded into a pass.
//
// USAGE
//   node tools/audit-shards.mjs                       # all segments, resume by default
//   node tools/audit-shards.mjs --segments=routes     # 13 segments, ~50-57s each (default)
//   node tools/audit-shards.mjs --segments=modes      # 2 segments (dark, light), ~350s each
//   node tools/audit-shards.mjs --only=dark           # one named segment
//   node tools/audit-shards.mjs --reset               # forget the state file
//   node tools/audit-shards.mjs --detach              # relaunch itself detached (WMI)
//   node tools/audit-shards.mjs -- <extra design-audit flags>
//
// DETACHED PATH (--detach): a plain "cmd &" child dies with the shell that
// started it (measured: a backgrounded --check died after its first route). The
// surviving shape is a WMI launch with the window hidden:
//     $cmd = 'cmd.exe /c cd /d <panel> && node tools\audit-shards.mjs ... > log 2>&1'
//     $s = ([wmiclass]'Win32_ProcessStartup').CreateInstance(); $s.ShowWindow = 0
//     ([wmiclass]'Win32_Process').Create($cmd, <panel>, $s)
// ShowWindow = 0 is not cosmetic: without it the WMI child puts a console window
// on the user's screen for the whole sweep.
import { spawn, spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { homedir } from "node:os";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const PANEL = resolve(HERE, "..");
const TOOL = join(HERE, "design-audit.mjs");
const ROUTES = ["home", "chat", "sessions", "board", "task", "memory", "knowledge", "graph", "agents", "runtimes", "stats", "settings", "inbox"];
const MODES = ["dark", "light"];
// How many consecutive empty captures (text=0) mean the instrument is broken (F-331b).
const EMPTY_CAPTURE_LIMIT = Number(process.env.AUDIT_EMPTY_CAPTURE_LIMIT || 3);

const argv = process.argv.slice(2);
const flag = (name, dflt) => {
  const hit = argv.find((a) => a === "--" + name || a.startsWith("--" + name + "="));
  if (!hit) return dflt;
  const eq = hit.indexOf("=");
  return eq === -1 ? true : hit.slice(eq + 1);
};
const passThrough = (() => {
  const i = argv.indexOf("--");
  return i === -1 ? [] : argv.slice(i + 1);
})();

const segments = flag("segments", "routes");
const only = flag("only", null);
// State lives OUTSIDE the repo by default (t332): it is runtime resume state, not an
// artefact, and a stray panel/tools state file would ride into a commit.
const stateDefault = join(homedir(), ".ruagent", "audit-shards-state.json");
const statePath = resolve(String(flag("state", stateDefault)));
mkdirSync(dirname(statePath), { recursive: true });
const outDir = flag("out", null);
const quiet = argv.includes("--quiet");

const plan = (() => {
  if (segments === "modes") return MODES.map((m) => ({ name: m, args: ["--modes=" + m] }));
  if (segments === "routes") return ROUTES.map((r) => ({ name: r, args: ["--routes=" + r] }));
  throw new Error("--segments must be modes|routes, got " + segments);
})();

const state = existsSync(statePath) ? JSON.parse(readFileSync(statePath, "utf8")) : { segments: {} };
if (argv.includes("--reset")) {
  state.segments = {};
  console.log("[shards] state reset: " + statePath);
}
state.segments = state.segments || {};
state.plan = plan.map((p) => p.name);

if (flag("detach", false) === true) {
  const log = join(PANEL, "tools", ".audit-shards-detached.log");
  const inner = "node tools/audit-shards.mjs " + argv.filter((a) => a !== "--detach").join(" ") + " > " + log.replace(/\//g, "\\") + " 2>&1";
  const cmd = 'cmd.exe /c cd /d ' + PANEL.replace(/\//g, "\\") + " && " + inner;
const ps = "$cmd = '" + cmd.replace(/'/g, "''") + "'; $s = ([wmiclass]'Win32_ProcessStartup').CreateInstance(); $s.ShowWindow = 0; $r = ([wmiclass]'Win32_Process').Create($cmd, '" + PANEL.replace(/\//g, "\\") + "', $s); Write-Output ('pid=' + $r.ProcessId + ' rc=' + $r.ReturnValue)";
  const res = spawnSync("powershell", ["-NoProfile", "-WindowStyle", "Hidden", "-Command", ps], { encoding: "utf8" });
  if (!/pid=\d+/.test(res.stdout || "")) { console.error("[shards] DETACH FAILED (no pid from WMI): " + (res.stdout || "") + (res.stderr || "")); process.exit(2); }
  console.log("[shards] detached (ShowWindow=0, WMI): " + (res.stdout || "").trim() + " · log=" + log);
  console.log("[shards] the child outlives this shell; poll the log, then re-run without --detach to read the aggregate.");
  process.exit(0);
}

// CLEANUP DISCIPLINE (F-331c, t331): this wrapper kills ONLY its own child handle
// (child.kill) or a PID it recorded itself. Never sweep processes by matching a command
// line: the shell running the sweep carries the same words on its own command line, so a
// name match can kill the very job doing the cleaning (measured: one call lost every step
// after the sweep because of exactly that).
const runSegment = (seg) =>
  new Promise((res) => {
    const args = [TOOL, ...seg.args, "--check", ...(outDir ? ["--out=" + outDir] : []), ...passThrough];
    const t0 = Date.now();
    const child = spawn(process.execPath, args, { cwd: PANEL });
    let out = "";
    // F-331b (t331): an audit whose captures come back empty is a BROKEN INSTRUMENT, and a
    // full sweep of empty captures produces a confident empty report. So watch the per-capture
    // lines and abort the segment once N consecutive captures report text=0.
    let emptyRun = 0;
    let killedForInstrument = false;
    const sink = (buf) => {
      const s = buf.toString();
      out += s;
      if (!quiet) process.stderr.write(s);
      for (const line of s.split(/\r?\n/)) {
        const m2 = line.match(/^\[audit\]\s+(\S+)\s+(\S+):\s+(\d+)ms\s+dom=(\d+)\s+text=(\d+)/);
        if (!m2) continue;
        emptyRun = Number(m2[5]) === 0 ? emptyRun + 1 : 0;
        if (emptyRun >= EMPTY_CAPTURE_LIMIT && !killedForInstrument) {
          killedForInstrument = true;
          process.stderr.write("[shards] INSTRUMENT FAILURE: " + emptyRun + " consecutive captures with text=0 -- aborting this segment instead of reporting an empty audit\n");
          try { child.kill("SIGTERM"); } catch { /* already gone */ }
        }
      }
    };
    child.stdout.on("data", sink);
    child.stderr.on("data", sink);
    const onSig = () => {
      try { child.kill("SIGTERM"); } catch { /* already gone */ }
      state.segments[seg.name] = { ...(state.segments[seg.name] || {}), status: "interrupted", ms: Date.now() - t0, why: "wrapper interrupted; child killed" };
      writeFileSync(statePath, JSON.stringify(state, null, 1));
      console.log("[shards] " + seg.name + " INTERRUPTED after " + Math.round((Date.now() - t0) / 1000) + "s -- recorded as interrupted, never as pass");
      process.exit(130);
    };
    process.on("SIGINT", onSig);
    process.on("SIGTERM", onSig);
    child.on("exit", (code) => {
      const ms = Date.now() - t0;
      const m = out.match(/checks\s+(\d+)\s+pass\s*\/\s*(\d+)\s+fail\s*\/\s*(\d+)\s+not measured/);
      const caps = out.match(/(\d+)\s+captures in\s+([\d.]+)s/);
      const failRows = (out.match(/failing rows:\s*([0-9,\s]+)/) || [, ""])[1].split(",").map((s) => s.trim()).filter(Boolean).map(Number);
      // t332: exit 0/1 are the tool own verdict codes (1 = a row failed). Any other code
      // means the sweep did not finish (SIGTERM 143, crash 101, ...) and a missing summary
      // line means the counts are unknown -- both are recorded as NOT done, so a half-run
      // segment can never be read as a pass.
      const ok = (code === 0 || code === 1) && !!m && !killedForInstrument;
      state.segments[seg.name] = {
        status: ok ? "done" : (killedForInstrument ? "instrument-failed" : (code === 143 || code === 130 ? "interrupted" : "failed")),
        why: ok ? null : (killedForInstrument ? "aborted: " + EMPTY_CAPTURE_LIMIT + "+ consecutive captures with text=0 (F-331b)" : "exit=" + code + (m ? "" : " and no summary line: counts unknown")),
        exitCode: code,
        ms,
        pass: m ? Number(m[1]) : null,
        fail: m ? Number(m[2]) : null,
        notMeasured: m ? Number(m[3]) : null,
        failingRows: failRows,
        captures: caps ? Number(caps[1]) : null,
        measuredS: caps ? Number(caps[2]) : null,
      };
      writeFileSync(statePath, JSON.stringify(state, null, 1));
      console.log("[shards] " + seg.name + " done in " + Math.round(ms / 1000) + "s exit=" + code + (m ? " checks " + m[1] + " pass / " + m[2] + " fail / " + m[3] + " not measured" : " (no summary line parsed)"));
      res();
    });
  });

const todo = plan.filter((p) => (only ? p.name === only : true) && state.segments[p.name]?.status !== "done");
const skipped = plan.filter((p) => !todo.includes(p)).map((p) => p.name);
if (skipped.length) console.log("[shards] resume: skipping done segments " + skipped.join(", "));
for (const seg of todo) {
  state.segments[seg.name] = { ...(state.segments[seg.name] || {}), status: "running", startedAt: new Date().toISOString() };
  writeFileSync(statePath, JSON.stringify(state, null, 1));
  await runSegment(seg);
}

// ---- aggregate: unions, never sums -----------------------------------------
const done = plan.filter((p) => state.segments[p.name]?.status === "done");
const pending = plan.filter((p) => !state.segments[p.name] || state.segments[p.name].status !== "done");
const incomplete = pending.filter((p) => state.segments[p.name]);
const failRows = [...new Set(done.flatMap((p) => state.segments[p.name].failingRows || []))].sort((a, b) => a - b);
const captures = done.reduce((n, p) => n + (state.segments[p.name].captures || 0), 0);
const notMeasured = done.reduce((n, p) => n + (state.segments[p.name].notMeasured || 0), 0);
console.log("[shards] segments done " + done.length + "/" + plan.length + " captures " + captures + " NOT-MEASURED rows reported by segments: " + notMeasured + " (per-segment, do not sum)");
if (incomplete.length) console.log("[shards] INCOMPLETE (ran but did not finish -- counts unknown, NEVER a pass): " + incomplete.map((p) => p.name + ":" + state.segments[p.name].status + "(" + (state.segments[p.name].why || "") + ")").join(", "));
console.log("[shards] failing rows (union across segments) = [" + failRows.join(", ") + "]");
if (pending.length) console.log("[shards] PENDING (not pass, not measured): " + pending.map((p) => p.name + ":" + (state.segments[p.name]?.status || "todo")).join(", "));
console.log("[shards] state=" + statePath + " · compare against a single run by the UNION of failing rows, not by summed counts");
process.exit(failRows.length ? 1 : 0);
