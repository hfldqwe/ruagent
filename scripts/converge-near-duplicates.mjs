#!/usr/bin/env node
// One-shot convergence of HISTORICAL near-duplicate memories (t330 / t334).
//
// Why this exists: t329 made the near-duplicate check able to see Chinese and
// turned a hit into a supersede, but it did not migrate history. The rows that
// were already written stay: 117 / 123 / 126 / 142 are four LIVE \`profile\` rows
// saying the same thing in different words. What t329 changed is that the store
// no longer GROWS; this script is the one-time collapse.
//
// Modes:
//   --dry-run   read-only. Prints the plan (which rows move, which row stays,
//               the resulting chain) and the polarity counter-evidence.
//   --execute   THE WRITE. Marks the older rows superseded (never deletes) in a
//               single transaction. Requires --yes, and the captain's gate.
//   --verify    re-scans afterwards and fails unless the live duplicate groups
//               are 0. Also prints the live/superseded counts and the chain.
//
// The criterion is a MIRROR of crates/memory/src/dedupe.rs. A second copy can
// drift, so the mirror carries the SAME vectors the Rust unit tests assert and
// refuses to run if any of them disagrees (see VECTORS / selfTest below).

import { DatabaseSync } from 'node:sqlite';
import { homedir } from 'node:os';
import { join } from 'node:path';

const DB = process.env.RUAGENT_DB ?? join(homedir(), '.ruagent', 'data', 'ruagent.db');
const args = process.argv.slice(2);
const has = (f) => args.includes(f);

// ---------------------------------------------------------------- criterion
// Keep in sync with crates/memory/src/dedupe.rs (IGNORABLE / POLARITY).
const IGNORABLE = [
  '偏好', '进行', '用户', '我们', '一种', '这个', '那个', '就是', '可以', '请',
  '的', '了', '呢', '啊', '吧', '吗', '我', '会', '要',
  'please', 'prefers', 'prefer', 'users', 'user', 'the', 'and', 'are', 'is',
  'to', 'of', 'in', 'a', 'an',
];
const POLARITY = [
  '不', '别', '没有', '没', '无需', '无', '禁止', '避免', '取消', '停止', '不再',
  '从不', '绝不', 'never', 'not', 'no', 'without', 'avoid', 'stop', 'dont',
  "don't", 'cannot', "can't",
];

const normalize = (s) => Array.from(s).filter((c) => /[\p{L}\p{N}]/u.test(c)).join('').toLowerCase();

// Longest common subsequence difference: characters of a not in its LCS with b.
// (Not a multiset walk — that returns leftovers in the wrong order and hands
// back fragments like 'ht' for 'the', which fail-closed then refuses.)
function lcsDiff(a, b) {
  const x = Array.from(a), y = Array.from(b);
  const dp = Array.from({ length: x.length + 1 }, () => new Array(y.length + 1).fill(0));
  for (let i = x.length - 1; i >= 0; i--) {
    for (let j = y.length - 1; j >= 0; j--) {
      dp[i][j] = x[i] === y[j] ? dp[i + 1][j + 1] + 1 : Math.max(dp[i + 1][j], dp[i][j + 1]);
    }
  }
  let i = 0, j = 0, out = '';
  while (i < x.length && j < y.length) {
    if (x[i] === y[j]) { i++; j++; }
    else if (dp[i + 1][j] >= dp[i][j + 1]) { out += x[i++]; }
    else { j++; }
  }
  while (i < x.length) out += x[i++];
  return out;
}

function stripIgnorable(s) {
  let out = s;
  for (const w of IGNORABLE) while (out.includes(w)) out = out.replace(w, '');
  return out;
}

function containsToken(haystack, token) {
  if (/[a-z]/.test(token)) return haystack.split(/[^\p{L}\p{N}]+/u).some((w) => w === token);
  return haystack.includes(token);
}

function judge(oldText, newText) {
  const a = normalize(oldText), b = normalize(newText);
  if (a === '' && b === '') return { verdict: 'Same' };
  if (a === b) return { verdict: 'Same' };
  if (a === '' || b === '') return { verdict: 'Refused', reason: 'one side is empty after normalisation' };
  const onlyOld = lcsDiff(a, b), onlyNew = lcsDiff(b, a);
  const changed = onlyOld + onlyNew;
  if (POLARITY.some((p) => containsToken(changed, p))) {
    return { verdict: 'Refused', reason: 'a polarity token is in the difference', onlyOld, onlyNew };
  }
  const leftOld = stripIgnorable(onlyOld), leftNew = stripIgnorable(onlyNew);
  if (leftOld === '' && leftNew === '') return { verdict: 'Mergeable', onlyOld, onlyNew };
  return { verdict: 'Refused', reason: 'a content word is in the difference', onlyOld, onlyNew };
}

// The Rust tests assert exactly these; if the mirror drifts, this script stops.
const VECTORS = [
  ['[distilled] 用户偏好使用简体中文交流。', '[distilled] 用户使用简体中文交流。', 'Mergeable'],
  ['[distilled] 用户使用简体中文交流。', '[distilled] 用户使用简体中文进行交流。', 'Mergeable'],
  ['[distilled] 用户偏好使用简体中文交流。', '[distilled] 偏好使用简体中文交流。', 'Mergeable'],
  ['用户偏好简体中文', '用户不使用简体中文', 'Refused'],
  ['[distilled] 用户偏好简体中文交流。', '[distilled] 用户偏好英文交流。', 'Refused'],
  ['the user prefers Rust and dislikes Java timezone UTC+8', 'user prefers Rust and dislikes Java timezone UTC+8', 'Mergeable'],
  ['the user prefers Rust and dislikes Java timezone UTC+8', 'User prefers Rust; dislikes Java (UTC+8)', 'Refused'],
];

function selfTest() {
  const bad = VECTORS.filter(([a, b, want]) => judge(a, b).verdict !== want);
  if (bad.length) {
    console.error('MIRROR DRIFT: the JS criterion disagrees with crates/memory/src/dedupe.rs on:');
    for (const [a, b, want] of bad) console.error(`  ${JSON.stringify(a)} vs ${JSON.stringify(b)} => ${judge(a, b).verdict}, expected ${want}`);
    process.exit(2);
  }
  return VECTORS.length;
}

// ------------------------------------------------------------------ database
const readOnly = !has('--execute');
const db = new DatabaseSync(DB, { readOnly });

function live(store, namespace) {
  return db.prepare(
    `SELECT id, store, namespace, content, created_at, source_episode, confidence
       FROM memories
      WHERE superseded_at IS NULL
        AND (?1 IS NULL OR store = ?1)
        AND (?2 IS NULL OR namespace = ?2)
      ORDER BY store, namespace, id`,
  ).all(store ?? null, namespace ?? null);
}

const prefix = (s, n = 28) => (s.length > n ? s.slice(0, n) + '…' : s);

// Group live rows of one (store, namespace) into components of Mergeable rows,
// then require a representative that is Mergeable with EVERY member. No
// representative means no plan for that component (fail-closed).
function planGroup(rows) {
  const parent = rows.map((_, i) => i);
  const find = (i) => (parent[i] === i ? i : (parent[i] = find(parent[i])));
  for (let i = 0; i < rows.length; i++) {
    for (let j = i + 1; j < rows.length; j++) {
      const v = judge(rows[i].content, rows[j].content).verdict;
      if (v === 'Mergeable' || v === 'Same') { parent[find(i)] = find(j); }
    }
  }
  const comps = new Map();
  rows.forEach((r, i) => {
    const root = find(i);
    if (!comps.has(root)) comps.set(root, []);
    comps.get(root).push(r);
  });
  const plans = [];
  for (const members of comps.values()) {
    if (members.length < 2) continue;
    // Keep the NEWEST row (highest id) that merges with all the others; that is
    // the shape t329's write path produces (#4 kept, #1..#3 superseded).
    const sorted = [...members].sort((a, b) => b.id - a.id);
    const keep = sorted.find((cand) => sorted.every((o) => o.id === cand.id || judge(o.content, cand.content).verdict !== 'Refused'));
    if (!keep) { plans.push({ members, keep: null }); continue; }
    const chain = [...members].sort((a, b) => a.id - b.id);
    plans.push({ members, keep, chain });
  }
  return plans;
}

function scan() {
  const rows = live(null, null);
  const byScope = new Map();
  for (const r of rows) {
    const k = `${r.store} / ${r.namespace}`;
    if (!byScope.has(k)) byScope.set(k, []);
    byScope.get(k).push(r);
  }
  const out = [];
  for (const [scope, group] of byScope) {
    for (const plan of planGroup(group)) out.push({ scope, ...plan });
  }
  return { rows, plans: out };
}

function polarityCounterEvidence(rows) {
  // Every pair the criterion REFUSED for polarity, so the plan can be read
  // together with the merges it did NOT make.
  const hits = [];
  for (let i = 0; i < rows.length; i++) {
    for (let j = i + 1; j < rows.length; j++) {
      if (rows[i].store !== rows[j].store || rows[i].namespace !== rows[j].namespace) continue;
      const v = judge(rows[i].content, rows[j].content);
      if (v.verdict === 'Refused' && /polarity/.test(v.reason ?? '')) {
        const a = normalize(rows[i].content), b = normalize(rows[j].content);
        // SYMMETRIC overlap: both difference sets count. One-sided overlap made
        // a long unrelated string look like a near neighbour -- the first
        // version reported 1236 polarity refusals, almost all of them noise.
        const overlap =
          1 - (lcsDiff(a, b).length + lcsDiff(b, a).length) / Math.max(1, a.length + b.length);
        hits.push({ a: rows[i], b: rows[j], reason: v.reason, onlyOld: v.onlyOld, onlyNew: v.onlyNew, overlap });
      }
    }
  }
  return hits.sort((x, y) => y.overlap - x.overlap);
}

// --------------------------------------------------------------------- modes
const vectors = selfTest();
console.log(`mirror self-test: ${vectors}/${vectors} vectors agree with crates/memory/src/dedupe.rs`);
console.log(`database: ${DB}${readOnly ? ' (read-only)' : ' (WRITE MODE)'}`);

const { rows, plans } = scan();
const profile = rows.filter((r) => r.store === 'profile');
console.log(`live rows: total=${rows.length} profile=${profile.length}`);
console.log(`live rows with source_episode: ${rows.filter((r) => r.source_episode !== null).length}`);

if (has('--dry-run') || !has('--execute')) {
  console.log('');
  console.log(`=== PLAN: ${plans.length} duplicate group(s) among live rows ===`);
  for (const p of plans) {
    if (!p.keep) {
      console.log(`  [${p.scope}] NO SAFE REPRESENTATIVE — not touched (${p.members.map((m) => '#' + m.id).join(', ')})`);
      continue;
    }
    console.log(`  [${p.scope}] keep #${p.keep.id}  ${prefix(p.keep.content)}`);
    console.log(`      keep:  id=${p.keep.id} created=${p.keep.created_at} source_episode=${p.keep.source_episode ?? 'NULL (historical, no source)'}`);
    for (const m of p.members.filter((m) => m.id !== p.keep.id).sort((a, b) => a.id - b.id)) {
      console.log(`      mark superseded: id=${m.id} created=${m.created_at} source_episode=${m.source_episode ?? 'NULL (historical, no source)'}  ${prefix(m.content)}`);
    }
    console.log(`      resulting chain: ${p.chain.map((m) => '#' + m.id).join(' <- ')}  (live row stays #${p.keep.id}, nothing is deleted)`);
    for (const m of p.members.filter((m) => m.id !== p.keep.id)) {
      const v = judge(m.content, p.keep.content);
      console.log(`      judgement #${m.id} vs #${p.keep.id}: ${v.verdict} (diff "${v.onlyOld}" / "${v.onlyNew}")`);
    }
  }
  console.log('');
  console.log('=== COUNTER-EVIDENCE: polarity differences that are NOT merged ===');
  const ce = polarityCounterEvidence(rows);
  console.log(`  pairs refused for polarity in the live store: ${ce.length}`);
  for (const h of ce.slice(0, 6)) {
    console.log(`  #${h.a.id} vs #${h.b.id} overlap=${(h.overlap * 100).toFixed(1)}% verdict=Refused(${h.reason}) diff "${h.onlyOld}" / "${h.onlyNew}"`);
    console.log(`      ${prefix(h.a.content, 34)}  ||  ${prefix(h.b.content, 34)}`);
  }
  console.log('  the contract pair, as a vector (the same input the Rust test uses):');
  const pair = ['[distilled] 用户偏好简体中文', '[distilled] 用户不使用简体中文'];
  const pv = judge(pair[0], pair[1]);
  console.log('    ' + JSON.stringify({ pair, verdict: pv.verdict, reason: pv.reason, only_old: pv.onlyOld, only_new: pv.onlyNew }));
  const f1 = '[distilled] 用户偏好使用简体中文交流。', f2 = '[distilled] 偏好使用简体中文交流。';
  const fv = judge(f1, f2);
  console.log('    ' + JSON.stringify({ four_phrasings: [f1, f2], verdict: fv.verdict, only_old: fv.onlyOld, only_new: fv.onlyNew }));
  console.log('  cosine note: this script does not embed (no model in node). The recorded');
  console.log('  numbers are t329\'s hash-embedder cosine 0.5000 for 用户偏好简体中文 vs');
  console.log('  用户不使用简体中文, and t325\'s live-model reading of 33 same-namespace');
  console.log('  pairs at cosine >= 0.95 — i.e. the neighbours this rule refuses to merge.');
  if (plans.length === 0) console.log('\n(no group to converge)');
}

if (has('--verify')) {
  const liveDupGroups = plans.length;
  const superseded = db.prepare('SELECT count(*) AS n FROM memories WHERE superseded_at IS NOT NULL').get().n;
  console.log('');
  console.log(`=== VERIFY ===`);
  console.log(`  live duplicate groups: ${liveDupGroups} (must be 0)`);
  console.log(`  live profile rows: ${profile.length} · superseded rows: ${superseded}`);
  if (liveDupGroups !== 0) {
    console.error(`FAIL: ${liveDupGroups} duplicate group(s) still live — convergence has not run (or did not cover them).`);
    process.exit(1);
  }
  console.log('  OK: no live row group is a near-duplicate of another.');
  const chain = db.prepare(
    `SELECT id, supersedes, superseded_at IS NOT NULL AS superseded, substr(content,1,24) AS c
       FROM memories WHERE supersedes IS NOT NULL OR superseded_at IS NOT NULL ORDER BY id`,
  ).all();
  console.log(`  chain rows (supersedes / superseded_at): ${chain.length}`);
  for (const r of chain) {
    console.log(`    #${r.id} supersedes=${r.supersedes ?? 'NULL'} superseded=${r.superseded} ${r.c}`);
  }
}

if (has('--execute')) {
  if (!has('--yes')) {
    console.error('refusing to write without --yes (this touches the user\'s real store)');
    process.exit(3);
  }
  const write = db.prepare(
    `UPDATE memories SET superseded_at = ?1, updated_at = ?1, supersedes = ?2 WHERE id = ?3 AND superseded_at IS NULL`,
  );
  const now = new Date().toISOString();
  let changed = 0;
  db.exec('BEGIN IMMEDIATE');
  try {
    for (const p of plans) {
      if (!p.keep) continue;
      // The CHAIN, written for real: every member points at the next-older one
      // (the oldest keeps NULL). superseded_at is set on all but the kept row,
      // and nothing is ever deleted.
      const ordered = [...p.members].sort((a, b) => a.id - b.id);
      ordered.forEach((m, i) => {
        const prev = i === 0 ? null : ordered[i - 1].id;
        if (m.id === p.keep.id) {
          if (prev !== null) {
            const r = db.prepare(`UPDATE memories SET supersedes = ?1, updated_at = ?2 WHERE id = ?3`).run(prev, now, m.id);
            changed += r.changes;
          }
        } else {
          const r = write.run(now, prev, m.id);
          changed += r.changes;
        }
      });
    }
    db.exec('COMMIT');
  } catch (e) {
    db.exec('ROLLBACK');
    throw e;
  }
  console.log(`executed: ${changed} row(s) marked superseded (none deleted), ${plans.length} group(s) converged`);
}
