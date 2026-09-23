#!/usr/bin/env node
// docs/design/check-contract.mjs —— 契约自检（design-lead 的产物）
// 归属与接口见 MASTER §12.10.5 第 6 条的「自检脚本的归属」：随契约一起演进，放远处必然腐化。
// 本脚本**不**调用审计工具（panel/tools/）：两边各自归属，由 tools 的双向对账连接。
//
// 用法：
//   node docs/design/check-contract.mjs              人类可读；非零退出码 = 失败
//   node docs/design/check-contract.mjs --json       机器可读（供 tools 对账调用）
//   node docs/design/check-contract.mjs --self-test  must-FAIL / must-PASS 自测（不读盘）
//   node docs/design/check-contract.mjs --help
//
// 五条规则（对应 t82 验收）：
//   1. 粘连（**行首定义**）：一行以 ---# 开头才算粘连。
//      修正史：原定义「在全文里搜 ---###」会命中**对该缺陷的提及**（写在行内代码片段里）—— 本脚本第一版就是这么误报的。
//   2. 标题层级完整性：#### a.b.c 必须有 ### a.b 父标题；### a.b 必须有 ## a；代码围栏必须成对闭合。
//   3. §12 表行连续：行号 1..N 无缺口，并输出 N（tools 对账时拿它比对工具的 n: 行数）。
//   4. 自描述计数：散文里的计数必须与**它所描述的结构条数**一致 —— 优选**派生**（由结构数出期望值，再与散文里的数字比对）。
//   5. 提及 vs 出现：作用于**文本**的判据必须先剥离**引用形态**（行内代码片段 / 代码块 / 引用块 / 注释）再判。
// JSON 形状（--json；注意 checks 嵌在 summary 下，不在顶层 —— 找错层级会得到与「不存在」相同的输出）：
//   { tool, version, ok, summary: { files, findings, checks: [glue, headings, table12, counts, pipes, numbering, mention-vs-occurrence] },
//     table12: { rows, max, gaps }, files: [ { path, findings: [] } ], findings: [ { path, id, severity, line, message } ] }
//      同一条理由的两个方向：行 44 是「读**源码**会误判」（⇒ 读 DOM），这里是「读**全文**会误判」（⇒ 按行首 / 结构定义）。

import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join, resolve, relative } from 'node:path';

const HERE = dirname(fileURLToPath(import.meta.url));
const DESIGN = HERE;                       // docs/design/
const REPO = resolve(HERE, '..', '..');

const CN = { '一': 1, '二': 2, '三': 3, '四': 4, '五': 5, '六': 6, '七': 7, '八': 8, '九': 9, '十': 10, '十一': 11, '十二': 12 };

// ---------- 规则 5：剥离「引用形态」（提及 vs 出现）----------
// 关键：**保留换行**（行号不能漂），只把引用里的内容抹掉。
export function maskRefs(raw) {
  const keepNl = (s) => s.replace(/[^\n]/g, '');
  return raw
    .replace(/`[^`\n]*`/g, (m) => keepNl(m))                 // 行内代码片段
    .replace(/```[\s\S]*?```/g, (m) => keepNl(m))           // 代码块
    .replace(/<!--[\s\S]*?-->/g, (m) => keepNl(m))          // HTML 注释
    .split('\n')
    .map((l) => (/^\s{0,3}>/.test(l) ? '' : l))               // 引用块整行
    .join('\n');
}

// ---------- 规则 1：粘连（行首定义）----------
export function checkGlue(masked) {
  const out = [];
  masked.split('\n').forEach((l, i) => {
    if (/^---#/.test(l)) out.push({ id: 'glue', severity: 'high', line: i + 1, message: '分隔线与标题粘连（行首 ---#）：' + l.slice(0, 48) });
  });
  return out;
}

// ---------- 规则 2：标题层级完整性 ----------
export function checkHeadings(masked) {
  const lines = masked.split('\n');
  const out = [];
  const fences = lines.filter((l) => /^\s*```/.test(l)).length;
  if (fences % 2 !== 0) out.push({ id: 'fence', severity: 'high', line: 0, message: '代码围栏未闭合（``` 行数为奇数：' + fences + '）' });
  const has = (p) => lines.some((l) => l.startsWith(p));
  lines.forEach((l, i) => {
    let m = /^####\s+(\d+)\.(\d+)\./.exec(l);
    if (m && !has('### ' + m[1] + '.' + m[2] + ' ')) {
      out.push({ id: 'hierarchy', severity: 'high', line: i + 1, message: '子标题缺父标题 ### ' + m[1] + '.' + m[2] });
    }
    m = /^###\s+(\d+)\.(\d+)\s/.exec(l);
    if (m && !has('## ' + m[1] + ' ')) {
      out.push({ id: 'hierarchy', severity: 'high', line: i + 1, message: '二级标题缺父标题 ## ' + m[1] });
    }
  });
  return out;
}

// ---------- 规则 3：§12 表行连续 ----------
export function checkTable12(masked) {
  const lines = masked.split('\n');
  let header = -1;
  for (let i = 0; i < lines.length; i++) {
    if (/^\|\s*#\s*\|\s*指标\s*\|/.test(lines[i])) { header = i; break; }
  }
  if (header < 0) return { findings: [{ id: 'table12', severity: 'medium', line: 0, message: '找不到 §12 表头（| # | 指标 | …）' }], rows: null };
  const nums = [];
  for (let i = header + 1; i < lines.length; i++) {
    if (!lines[i].startsWith('|')) break;
    const m = /^\|\s*(\d+)\s*\|/.exec(lines[i]);
    if (m) nums.push(parseInt(m[1], 10));
  }
  const max = nums.length ? Math.max.apply(null, nums) : 0;
  const gaps = [];
  for (let n = 1; n <= max; n++) if (nums.indexOf(n) < 0) gaps.push(n);
  const findings = [];
  if (gaps.length) findings.push({ id: 'table12', severity: 'high', line: header + 1, message: '§12 表行号有缺口：' + JSON.stringify(gaps) + '（现有 ' + nums.length + ' 行，最大 ' + max + '）' });
  return { findings, rows: { rows: nums.length, max: max, gaps: gaps } };
}

// ---------- 规则 4：自描述计数（派生后断言）----------
function deriveNumberedItems(lines, at) {
  let n = 0;
  for (let i = at; i < lines.length; i++) {
    if (/^#{2,4}\s/.test(lines[i])) break;
    if (/^\d+\.\s+\*\*/.test(lines[i])) n++;
  }
  return n;
}
function derivePrecedingTableRows(lines, at) {
  // 最近的**前一张表**：从短语往上找，跳过非表行（短语与表之间可能隔着别的段落）
  // 最近的**前一张表**：从短语往上找，**到本节标题为止**（
  // 不用「120 行」这种魔数 —— 那会让插入内容悄悄把表挤出窗口，而报出「无法派生」）
  let end = -1;
  for (let i = at - 1; i >= 0; i--) {
    if (/^#{2,4}\s/.test(lines[i])) break;      // 本节开头 ⇒ 停
    if (lines[i].startsWith('|')) { end = i; break; }
  }
  if (end < 0) return null;
  let start = end;
  while (start >= 0 && lines[start].startsWith('|')) start--;
  const block = lines.slice(start + 1, end + 1);
  return block.filter((l, i) => i !== 0 && !/^\|[\s:|-]+\|$/.test(l)).length;
}
export function checkCounts(masked) {
  const lines = masked.split('\n');
  const out = [];
  const rules = [
    { id: 'count-items', re: /^\*\*另有(十一|十二|一|二|三|四|五|六|七|八|九|十)类问题/, derive: deriveNumberedItems, what: '其后紧跟的编号条目数' },
    { id: 'count-examples', re: /^\*\*(十一|十二|一|二|三|四|五|六|七|八|九|十)例的修法都不动数值\*\*/, derive: derivePrecedingTableRows, what: '其前一张表的数据行数' },
  ];
  for (const r of rules) {
    lines.forEach((l, i) => {
      const m = r.re.exec(l);
      if (!m) return;
      const claimed = CN[m[1]];
      const actual = r.derive(lines, i);
      if (actual == null) {
        out.push({ id: r.id, severity: 'medium', line: i + 1, message: '自描述计数无法派生（' + r.what + ' 找不到结构）' });
      } else if (claimed !== actual) {
        out.push({ id: r.id, severity: 'high', line: i + 1, message: '自描述计数与结构不符：散文写「' + m[1] + '」（' + claimed + '），' + r.what + ' = ' + actual + ' ⇒ 计数必须派生，不得手写' });
      }
    });
  }
  return out;
}

// ---------- 规则 6：表格列数一致（表格里的裸 | 会切断单元格）----------
export function checkPipes(masked) {
  const lines = masked.split('\n');
  const BS = String.fromCharCode(92);
  const NUL = String.fromCharCode(0);
  const maskEsc = (l) => l.split(BS + '|').join(NUL);
  const nPipes = (l) => maskEsc(l).split('|').length;
  const out = [];
  let i = 0;
  while (i < lines.length) {
    if (lines[i].startsWith('|') && i + 1 < lines.length && lines[i + 1].startsWith('|')) {
      const sep = lines[i + 1].replace(/[|\s:-]/g, '');
      if (sep === '') {
        const want = nPipes(lines[i]);
        let j = i;
        while (j < lines.length && lines[j].startsWith('|')) {
          if (nPipes(lines[j]) !== want) {
            out.push({ id: 'pipes', severity: 'high', line: j + 1, message: '表格列数不一致：' + nPipes(lines[j]) + ' vs 表头 ' + want + '（单元格里的裸 | 要写成 \\| ）' });
          }
          j++;
        }
        i = j; continue;
      }
    }
    i++;
  }
  return out;
}

// ---------- 规则 7：编号连续（编号列表从 1 起就必须 1..N 递增，不得跳号/重号）----------
export function checkNumbering(masked) {
  const lines = masked.split('\n');
  const out = [];
  let i = 0;
  while (i < lines.length) {
    if (/^\d+\.\s+\*\*/.test(lines[i])) {
      const start = i;
      const nums = [];
      while (i < lines.length && /^\d+\.\s+\*\*/.test(lines[i])) {
        nums.push(parseInt(/^(\d+)\./.exec(lines[i])[1], 10));
        i++;
      }
      const want = nums.map((_, k) => k + 1);
      if (nums[0] === 1 && JSON.stringify(nums) !== JSON.stringify(want)) {
        out.push({ id: 'numbering', severity: 'high', line: start + 1, message: '编号不连续 / 不递增：' + JSON.stringify(nums) + '（应为 ' + JSON.stringify(want) + '）' });
      }
      continue;
    }
    i++;
  }
  return out;
}

// ---------- 汇总 ----------
export function runChecks(raw, opts) {
  const isMaster = !!(opts && opts.isMaster);
  const masked = maskRefs(raw);
  const t12 = isMaster ? checkTable12(masked) : { findings: [], rows: null };
  const findings = [].concat(checkGlue(masked), checkHeadings(masked), t12.findings, checkCounts(masked), checkPipes(masked), checkNumbering(masked));
  return { findings, rows: t12.rows };
}

export function targets() {
  const list = [join(DESIGN, 'MASTER.md'), join(DESIGN, 'primitives.md')];
  const views = join(DESIGN, 'views');
  for (const f of readdirSync(views).sort()) if (f.endsWith('.md')) list.push(join(views, f));
  return list;
}

function main(argv) {
  if (argv.includes('--help')) {
    // help == 文件头的注释块（自动同步：改头注释即改 help；跳过 shebang 行）
    const out = [];
    const src = readFileSync(fileURLToPath(import.meta.url), 'utf8').split(String.fromCharCode(10)).slice(1);
    for (const l of src) {
      if (!l.startsWith('//')) break;
      out.push(l);
    }
    console.log(out.join(String.fromCharCode(10)));
    return 0;
  }
  if (argv.includes('--self-test')) return selfTest();
  const asJson = argv.includes('--json');
  const files = [];
  const all = [];
  let masterRows = null;
  for (const p of targets()) {
    const raw = readFileSync(p, 'utf8');
    const rel = relative(REPO, p).split('\\').join('/');
    const r = runChecks(raw, { isMaster: rel.endsWith('docs/design/MASTER.md') });
    if (rel.endsWith('docs/design/MASTER.md')) masterRows = r.rows;
    files.push({ path: rel, findings: r.findings });
    for (const f of r.findings) all.push(Object.assign({ path: rel }, f));
  }
  const ok = all.length === 0;
  if (asJson) {
    console.log(JSON.stringify({ tool: 'docs/design/check-contract.mjs', version: 1, ok: ok, summary: { files: files.length, findings: all.length, checks: ['glue', 'headings', 'table12', 'counts', 'pipes', 'numbering', 'mention-vs-occurrence'] }, table12: masterRows, files: files, findings: all }, null, 2));
  } else {
    for (const f of files) {
      if (!f.findings.length) { console.log('  OK   ' + f.path); continue; }
      for (const x of f.findings) console.log('  FAIL ' + f.path + ':' + x.line + '  [' + x.id + '] ' + x.message);
    }
    console.log('');
    console.log('§12 表：' + (masterRows ? masterRows.rows + ' 行，最大 ' + masterRows.max + '，缺口 ' + JSON.stringify(masterRows.gaps) : '未找到'));
    console.log('CONTRACT SELF-CHECK: ' + (ok ? 'PASS' : 'FAIL (' + all.length + ' findings)'));
  }
  return ok ? 0 : 1;
}

// ---------- must-FAIL / must-PASS 自测 ----------
function selfTest() {
  const clean = '# T\n\n## 1 A\n\n### 1.1 B\n\n正文。\n';
  const countBad = '# T\n\n## 1 A\n\n**另有三类问题**\n\n1. **a** —— x\n2. **b** —— y\n';
  const glue = '# T\n\n## 1 A\n\n### 1.1 B\n\n---### 1.2 C\n';
  const rowGap = '# T\n\n## 1 A\n\n| # | 指标 | 基线数字 | 目标数字 | 判定方法 |\n|---|---|---|---|---|\n| 1 | a | x | y | z |\n| 2 | b | x | y | z |\n| 4 | d | x | y | z |\n';
  const mentionOnly = '# T\n\n## 1 A\n\n### 1.1 B\n\n正文里提到 `---### 1.2`（行内代码片段里的**提及**，不是出现）。\n';
  const mentionPlusDefect = '# T\n\n## 1 A\n\n### 1.1 B\n\n**另有三类问题**\n\n1. **a** —— x\n2. **b** —— y\n\n正文提到 @---### 1.2@（提及，必须被剥离）。\n';
  const mentionUnquoted = '# T\n\n## 1 A\n\n### 1.1 B\n\n---### 1.2 C\n';
  const cases = [
    { name: 'clean（无缺陷）', doc: clean, expect: [] },
    { name: 'must-FAIL ① 计数与结构不符（另有三类 vs 实际 2 条）', doc: countBad, expect: ['count-items'] },
    { name: 'must-FAIL ② 父标题粘连（---### 1.2）', doc: glue, expect: ['glue'] },
    { name: 'must-FAIL ③ §12 表行缺口（1,2,4）', doc: rowGap, expect: ['table12'], isMaster: true },
    { name: 'must-FAIL ⑥ 子标题缺父标题（#### 1.1.1 无 ### 1.1）', doc: '# T\n\n## 1 A\n\n#### 1.1.1 X\n', expect: ['hierarchy'] },
    { name: 'must-PASS ④a 「提及」不是「出现」（提及写在行内代码片段里）', doc: mentionOnly, expect: [] },
    { name: 'must-FAIL ⑦ 提及与真缺陷并存 ⇒ 必须抓到真缺陷（遮蔽没有过度遮蔽）', doc: mentionPlusDefect, expect: ['count-items'] },
    { name: 'must-FAIL ⑧ 表格列数不一致（单元格里裸 |）', doc: '# T\n\n## 1 A\n\n| a | b |\n|---|---|\n| x | y |\n| p | q | r |\n', expect: ['pipes'] },
    { name: 'must-FAIL ⑨ 编号跳号（1,2,4）', doc: '# T\n\n## 1 A\n\n1. **a** —— x\n2. **b** —— y\n4. **d** —— z\n', expect: ['numbering'] },
    { name: 'must-FAIL ④b 同一文本去掉引用形态（真出现）', doc: mentionUnquoted, expect: ['glue'] },
  ];
  let bad = 0;
  console.log('self-test（must-FAIL / must-PASS）');
  for (const c of cases) {
    const got = runChecks(c.doc, { isMaster: !!c.isMaster }).findings.map((f) => f.id).sort();
    const want = c.expect.slice().sort();
    const ok = JSON.stringify(got) === JSON.stringify(want);
    if (!ok) bad++;
    console.log('  ' + (ok ? 'OK  ' : 'FAIL') + ' ' + c.name + ' → 期望 ' + JSON.stringify(want) + '，实测 ' + JSON.stringify(got));
  }
  console.log('');
  console.log('SELF-TEST: ' + (bad ? 'FAIL (' + bad + ' 项不符)' : 'PASS（' + cases.length + '/' + cases.length + '）'));
  return bad ? 1 : 0;
}

// 入口守卫：被 import 时不执行 CLI（tools 的对账可能直接 import 本模块的 runChecks）
const ENTRY = process.argv[1] ? resolve(process.argv[1]) : '';
if (ENTRY === fileURLToPath(import.meta.url)) process.exit(main(process.argv.slice(2)));

