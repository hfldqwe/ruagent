//! ruagent CLI: thin client over the daemon's local API (design §3).

use std::io::BufRead;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ruagent",
    version,
    about = "Local-first agent engineering platform"
)]
struct Cli {
    /// Base URL of a running daemon.
    #[arg(long, env = "RUAGENT_URL", default_value = "http://127.0.0.1:8787")]
    url: String,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start the resident daemon.
    Serve {
        /// Listen address.
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: String,
        /// Data root (default: ~/.ruagent).
        #[arg(long, env = "RUAGENT_HOME")]
        root: Option<std::path::PathBuf>,
    },
    /// List registered agents.
    Agents,
    /// List tasks.
    Status,
    /// Run the platform MCP server over stdio (spawned by agents; talks
    /// to the daemon at RUAGENT_URL).
    McpServe,
    /// List discovered skills (platform + project).
    Skills,
    /// Sync skills into every enabled harness's skill directories.
    SkillsSync,
    /// Platform self-check: embedder, semantic recall, memory lifecycle,
    /// graph traversal, session sync, distillation. Prints PASS/FAIL
    /// per check with details; exit code 1 if anything failed. Every
    /// probe artifact lives in a reserved probe space, and the removable
    /// one is deleted before the report (t252).
    Doctor {
        /// Also sweep probe documents left behind by older doctor runs.
        #[arg(long)]
        cleanup: bool,
    },
    /// Run a prompt as a one-off task and stream the result.
    Run {
        prompt: String,
        /// Agent name (default: first enabled agent).
        #[arg(long)]
        agent: Option<String>,
    },
    /// Knowledge base: ingest a directory of markdown into the index.
    Knowledge {
        #[command(subcommand)]
        cmd: KnowledgeCmd,
    },
    /// Wiki mode: compile source documents into interlinked pages.
    Wiki {
        #[command(subcommand)]
        cmd: WikiCmd,
    },
}

#[derive(Subcommand)]
enum WikiCmd {
    /// Compile source documents into wiki pages. Default is a dry run:
    /// prints the plan, writes nothing -- confirm with --confirm <id>.
    Build {
        /// Scope: "all", "changed", or comma-separated source names.
        #[arg(long, default_value = "changed")]
        scope: String,
        /// Plan only -- print the page plan, write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Execute a reviewed dry-run plan by build id.
        #[arg(long)]
        confirm: Option<i64>,
        /// Agent name (default: dsh, else first enabled).
        #[arg(long)]
        agent: Option<String>,
    },
    /// List wiki pages with stale/edited markers and link counts.
    List,
    /// Print the link graph: edges, broken (wanted) pages, orphans.
    Links,
    /// Print one page's raw markdown.
    Show { slug: String },
}

#[derive(Subcommand)]
enum KnowledgeCmd {
    /// Ingest every `.md` under DIR as a searchable document (nested names
    /// are preserved), printing bytes and chunk counts per file plus a
    /// before/after total: the corpus stops being empty by accident.
    Ingest {
        /// Directory to walk recursively for `.md` files.
        dir: std::path::PathBuf,
        /// Document-name prefix (default: the directory's own name), so
        /// `docs/design/MASTER.md` becomes `docs/design/MASTER`.
        #[arg(long)]
        prefix: Option<String>,
        /// Print the plan; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Stop after N files (0 = every file).
        #[arg(long, default_value_t = 0)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { addr, root } => {
            let addr: std::net::SocketAddr = addr.parse().context("invalid listen address")?;
            let root = root.unwrap_or_else(ruagent_daemon::default_root);
            // Only serve needs a runtime; the other subcommands are sync
            // (reqwest::blocking's internal runtime must never be dropped
            // inside an async context).
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("building tokio runtime")?;
            runtime.block_on(ruagent_daemon::serve(root, addr))
        }
        Cmd::Agents => list_agents(&cli.url),
        Cmd::Doctor { cleanup } => doctor(&cli.url, cleanup),
        Cmd::Status => status(&cli.url),
        Cmd::Skills => skills_cmd(&cli.url, false),
        Cmd::SkillsSync => skills_cmd(&cli.url, true),
        Cmd::McpServe => {
            // serve_stdio is async: drive it on its own runtime (the rest
            // of the CLI is sync for reqwest::blocking, see M1 lessons).
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("building tokio runtime")?;
            runtime
                .block_on(ruagent_mcp::serve_stdio())
                .map_err(|e| anyhow::anyhow!("{e}"))
        }
        Cmd::Run { prompt, agent } => run(&cli.url, &prompt, agent.as_deref()),
        Cmd::Wiki { cmd } => wiki_cmd(&cli.url, cmd),
        Cmd::Knowledge { cmd } => knowledge_cmd(&cli.url, cmd),
    }
}

// ---------------------------------------------------------------------------
// wiki: compile sources into interlinked pages (thin client over the API)
// ---------------------------------------------------------------------------

/// Response -> JSON with the daemon's {message} error body surfaced.
fn api_json(resp: reqwest::blocking::Response) -> Result<serde_json::Value> {
    let status = resp.status();
    let text = resp.text().context("reading response")?;
    if !status.is_success() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(m) = v["message"].as_str()
        {
            bail!("{m}");
        }
        bail!("HTTP {status}: {text}");
    }
    serde_json::from_str(&text).context("parsing response")
}

fn wiki_cmd(url: &str, cmd: WikiCmd) -> Result<()> {
    match cmd {
        WikiCmd::Build {
            scope,
            dry_run,
            confirm,
            agent,
        } => {
            let _ = dry_run; // plain builds always plan first (the gate)
            wiki_build(url, &scope, confirm, agent.as_deref())
        }
        WikiCmd::List => wiki_list(url),
        WikiCmd::Links => wiki_links(url),
        WikiCmd::Show { slug } => wiki_show(url, &slug),
    }
}

fn wiki_build(url: &str, scope: &str, confirm: Option<i64>, agent: Option<&str>) -> Result<()> {
    let body = if let Some(id) = confirm {
        serde_json::json!({ "confirm_plan": id, "agent": agent })
    } else {
        // "all"/"changed" pass through; anything else is a name list.
        // Always dry_run: the plan-level human gate -- confirm explicitly.
        let scope_json = if scope == "all" || scope == "changed" {
            serde_json::json!(scope)
        } else {
            serde_json::json!(
                scope
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            )
        };
        serde_json::json!({ "scope": scope_json, "dry_run": true, "agent": agent })
    };
    let is_plan = confirm.is_none();
    let resp = api_json(
        client()
            .post(format!("{url}/api/v1/knowledge/wiki/build"))
            .json(&body)
            .send()
            .context("daemon unreachable (is `ruagent serve` running?)")?,
    )?;
    let build_id = resp["build_id"].as_i64().unwrap_or(0);
    let planned = resp["pages_planned"].as_i64().unwrap_or(0);
    let agent_name = resp["agent"].as_str().unwrap_or("?");
    if is_plan {
        println!("plan #{build_id} by {agent_name} -- {planned} pages (dry run, nothing written)");
        if let Some(notes) = resp["notes"].as_str() {
            println!("notes: {notes}");
        }
        for p in resp["plan"].as_array().unwrap_or(&Vec::new()) {
            let sources = p["sources"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            println!(
                "  [{:<6}] {:<24} {} (sources: {})",
                p["action"].as_str().unwrap_or("?"),
                p["slug"].as_str().unwrap_or("?"),
                p["title"].as_str().unwrap_or(""),
                sources,
            );
        }
        println!();
        println!("confirm to execute: ruagent wiki build --confirm {build_id}");
    } else {
        println!("build #{build_id} running via {agent_name} -- {planned} pages planned");
        println!("watch progress in the panel (知识库 -> Wiki) or `ruagent wiki list`");
    }
    Ok(())
}

fn wiki_list(url: &str) -> Result<()> {
    let resp = api_json(
        client()
            .get(format!("{url}/api/v1/knowledge/wiki/pages"))
            .send()
            .context("daemon unreachable")?,
    )?;
    let pages = resp["pages"].as_array().context("bad response")?;
    if pages.is_empty() {
        println!("no wiki pages -- `ruagent wiki build` to compile some");
        return Ok(());
    }
    println!(
        "{:<5} {:<26} {:<30} {:<7} {:<6} SOURCES",
        "MARK", "SLUG", "TITLE", "OUT/IN", "STALE"
    );
    for p in pages {
        let mut mark = String::new();
        if p["stale"].as_bool().unwrap_or(false) {
            mark.push('S');
        }
        if p["edited"].as_bool().unwrap_or(false) {
            mark.push('E');
        }
        let title: String = p["title"].as_str().unwrap_or("").chars().take(28).collect();
        let sources = p["sources"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .unwrap_or_default();
        println!(
            "{:<5} {:<26} {:<30} {:<3}/{:<3}  {:<6} {}",
            mark,
            p["slug"].as_str().unwrap_or("?"),
            title,
            p["links_out"].as_i64().unwrap_or(0),
            p["links_in"].as_i64().unwrap_or(0),
            if p["stale"].as_bool().unwrap_or(false) {
                "stale"
            } else {
                ""
            },
            sources,
        );
    }
    println!();
    println!("S = sources updated since generation, E = hand-edited (builds skip it)");
    Ok(())
}

fn wiki_links(url: &str) -> Result<()> {
    let resp = api_json(
        client()
            .get(format!("{url}/api/v1/knowledge/wiki/links"))
            .send()
            .context("daemon unreachable")?,
    )?;
    let edges = resp["edges"].as_array().context("bad response")?;
    let broken = resp["broken"].as_array().context("bad response")?;
    let orphans = resp["orphans"].as_array().context("bad response")?;
    println!("edges ({}):", edges.len());
    for e in edges {
        println!(
            "  {} -> {}",
            e["src"].as_str().unwrap_or("?"),
            e["dst"].as_str().unwrap_or("?")
        );
    }
    let wanted: Vec<&str> = broken.iter().filter_map(|b| b.as_str()).collect();
    println!();
    println!(
        "wanted (linked but missing, {}): {}",
        wanted.len(),
        wanted.join(", ")
    );
    let orph: Vec<&str> = orphans.iter().filter_map(|o| o.as_str()).collect();
    println!("orphans ({}): {}", orph.len(), orph.join(", "));
    Ok(())
}

fn wiki_show(url: &str, slug: &str) -> Result<()> {
    let name = format!("wiki/{slug}");
    let resp = client()
        .get(format!(
            "{url}/api/v1/knowledge/raw/{}",
            urlencoding::encode(&name)
        ))
        .send()
        .context("daemon unreachable")?;
    let status = resp.status();
    let text = resp.text().context("reading response")?;
    if !status.is_success() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(m) = v["message"].as_str()
        {
            bail!("{m}");
        }
        bail!("HTTP {status}: {text}");
    }
    print!("{text}");
    Ok(())
}

/// The probe space `ruagent doctor` may write into. ONE list: the three
/// payloads, the recall URL and `sweep_probe_artifacts` all read these
/// constants, so a rename cannot orphan a class of artifact, and the sweep
/// cannot drift from the writer (t252/t282, single-source rule).
const PROBE_DOC_NAME: &str = "__probe__/doctor-probe";
const PROBE_DOC_NAME_LEGACY: &str = "doctor-probe";
const PROBE_ENTITY_NAME: &str = "__probe__doctor-node";
const PROBE_ENTITY_NAME_LEGACY: &str = "doctor-node";
const PROBE_MEMORY_NAMESPACE: &str = "agent:__probe__";
/// The probe memory's content. Shared by the writer and the legacy sweep: the
/// pre-t252 doctor wrote it into the `user` namespace, where the injection
/// paths read it, so the sweep must recognise it by content.
const PROBE_MEMORY_CONTENT: &str = "doctor probe: the kettle is chrome";

/// The doctor's recall probe, declaring itself: a probe that does not say
/// so is indistinguishable from a user query, and guessing from the query
/// text is not a reading (t251 / t252).
const DOCTOR_RECALL_URL: &str =
    "/api/v1/recall?q=kettle%20material&strategy=aggressive&top_n=3&source=probe";

/// Keep the first occurrence of each id: the sweeps above can name the same
/// row twice from different angles, and acting twice turns a successful cleanup
/// into a reported failure (t282).
fn dedupe_by_id(targets: &mut Vec<(i64, String)>) {
    let mut seen = std::collections::HashSet::new();
    targets.retain(|(id, _)| seen.insert(*id));
}

/// GET one JSON document; the error is a STRING the caller prints, because a
/// sweep that cannot list must say so instead of looking clean.
fn get_json(url: &str, path: &str) -> Result<serde_json::Value, String> {
    client()
        .get(format!("{url}{path}"))
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json::<serde_json::Value>())
        .map_err(|e| e.to_string())
}

/// Delete every artifact `ruagent doctor` (or an older build of it) left behind
/// — documents, entities AND memories — printing each failure. All three
/// classes are named by the constants above, so a rename cannot orphan one of
/// them (t252/t282). Returns (nothing left behind, human-readable lines).
fn sweep_probe_artifacts(url: &str, include_legacy: bool) -> (bool, Vec<String>) {
    let mut lines = Vec::new();
    let mut left = 0usize;

    // 1. Documents: the `.md` file goes with the row.
    let doc_names: Vec<&str> = if include_legacy {
        vec![PROBE_DOC_NAME, PROBE_DOC_NAME_LEGACY]
    } else {
        vec![PROBE_DOC_NAME]
    };
    match get_json(url, "/api/v1/knowledge/documents") {
        Ok(v) => {
            for row in v["documents"].as_array().cloned().unwrap_or_default() {
                let name = row["name"].as_str().unwrap_or_default().to_string();
                if !doc_names.contains(&name.as_str()) {
                    continue;
                }
                let id = row["id"].as_i64().unwrap_or(0);
                match client()
                    .delete(format!("{url}/api/v1/knowledge/documents/{id}"))
                    .send()
                {
                    Ok(r) if r.status().is_success() => {
                        lines.push(format!("removed probe document {name} (#{id})"))
                    }
                    Ok(r) => {
                        left += 1;
                        lines.push(format!(
                            "FAILED to remove document {name} (#{id}): HTTP {}",
                            r.status()
                        ))
                    }
                    Err(e) => {
                        left += 1;
                        lines.push(format!("FAILED to remove document {name} (#{id}): {e}"))
                    }
                }
            }
        }
        Err(e) => {
            left += 1;
            lines.push(format!("could not list documents to sweep: {e}"));
        }
    }

    // 2. Entities: hard delete — the edges/facts go with the row (t276).
    let entity_names: Vec<&str> = if include_legacy {
        vec![PROBE_ENTITY_NAME, PROBE_ENTITY_NAME_LEGACY]
    } else {
        vec![PROBE_ENTITY_NAME]
    };
    let mut entity_targets: Vec<(i64, String)> = Vec::new();
    for name in &entity_names {
        let path = format!("/api/v1/graph/search?q={}", urlencoding::encode(name));
        match get_json(url, &path) {
            Ok(v) => {
                for row in v["entities"].as_array().cloned().unwrap_or_default() {
                    // Exact match only: the entity search leg is loose on
                    // purpose, and deleting a near-match would be a bug.
                    if row["name"].as_str() != Some(*name) {
                        continue;
                    }
                    if let Some(id) = row["id"].as_i64() {
                        entity_targets.push((id, (*name).to_string()));
                    }
                }
            }
            Err(e) => {
                left += 1;
                lines.push(format!("could not search entities to sweep: {e}"));
            }
        }
    }
    // One id can match twice (the search leg is run per name); a second
    // DELETE would 404 and be reported as a failure of a sweep that in fact
    // worked. Dedupe before acting (t282).
    dedupe_by_id(&mut entity_targets);
    for (id, name) in entity_targets {
        match client()
            .delete(format!("{url}/api/v1/graph/entity/{id}"))
            .send()
        {
            Ok(r) if r.status().is_success() => {
                lines.push(format!("removed probe entity {name} (#{id})"))
            }
            Ok(r) => {
                left += 1;
                lines.push(format!(
                    "FAILED to remove entity {name} (#{id}): HTTP {}",
                    r.status()
                ))
            }
            Err(e) => {
                left += 1;
                lines.push(format!("FAILED to remove entity {name} (#{id}): {e}"))
            }
        }
    }

    // 3. Memories: hard PURGE — a tombstone would keep the content (t276).
    let mut memory_targets: Vec<(i64, String)> = Vec::new();
    let ns_path = format!(
        "/api/v1/memory/list?namespace={}&limit=500",
        urlencoding::encode(PROBE_MEMORY_NAMESPACE)
    );
    match get_json(url, &ns_path) {
        Ok(v) => {
            for row in v["memories"].as_array().cloned().unwrap_or_default() {
                if let Some(id) = row["id"].as_i64() {
                    memory_targets.push((id, format!("namespace {PROBE_MEMORY_NAMESPACE}")));
                }
            }
        }
        Err(e) => {
            left += 1;
            lines.push(format!("could not list probe-namespace memories: {e}"));
        }
    }
    if include_legacy {
        // The pre-t252 doctor wrote its probe into the `user` namespace, where
        // the injection paths read it: sweep by CONTENT, from the same
        // constant the payload is built from.
        match get_json(url, "/api/v1/memory/list?limit=500") {
            Ok(v) => {
                for row in v["memories"].as_array().cloned().unwrap_or_default() {
                    let content = row["content"].as_str().unwrap_or_default();
                    if content.trim() != PROBE_MEMORY_CONTENT {
                        continue;
                    }
                    if let Some(id) = row["id"].as_i64() {
                        memory_targets.push((id, "legacy probe content".to_string()));
                    }
                }
            }
            Err(e) => {
                left += 1;
                lines.push(format!("could not list memories to sweep: {e}"));
            }
        }
    }
    // Same trap as the entities: #164 is listed by BOTH the namespace sweep
    // and the legacy-content sweep, and purging it twice reports a bogus 404.
    dedupe_by_id(&mut memory_targets);
    for (id, why) in memory_targets {
        match client()
            .delete(format!("{url}/api/v1/memory/{id}?purge=true"))
            .send()
        {
            Ok(r) if r.status().is_success() => {
                lines.push(format!("purged probe memory #{id} ({why})"))
            }
            Ok(r) => {
                left += 1;
                lines.push(format!(
                    "FAILED to purge memory #{id} ({why}): HTTP {}",
                    r.status()
                ))
            }
            Err(e) => {
                left += 1;
                lines.push(format!("FAILED to purge memory #{id} ({why}): {e}"))
            }
        }
    }

    if lines.is_empty() {
        lines.push("no probe artifact present".into());
    }
    (left == 0, lines)
}

fn knowledge_cmd(url: &str, cmd: KnowledgeCmd) -> Result<()> {
    match cmd {
        KnowledgeCmd::Ingest {
            dir,
            prefix,
            dry_run,
            limit,
        } => ingest_dir(url, &dir, prefix, dry_run, limit),
    }
}

/// Ingest every `.md` under `dir` through the public API, printing bytes and
/// chunk counts per file plus a before/after total: "the corpus is empty"
/// stops being a guess (t252). Failures are counted and reported, never
/// skipped silently.
fn ingest_dir(
    url: &str,
    dir: &std::path::Path,
    prefix: Option<String>,
    dry_run: bool,
    limit: usize,
) -> Result<()> {
    let prefix = prefix.unwrap_or_else(|| {
        dir.file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    });
    let md = ruagent_knowledge::files::walk_markdown(dir);
    if md.is_empty() {
        bail!("no .md files under {}", dir.display());
    }
    let other = count_non_md(dir);
    let files: Vec<&std::path::PathBuf> = if limit > 0 {
        md.iter().take(limit).collect()
    } else {
        md.iter().collect()
    };
    let before = documents_snapshot(url);
    let mut bytes = 0usize;
    let mut chunks = 0usize;
    let mut failed: Vec<(String, String)> = Vec::new();
    for f in &files {
        let Some(name) = ruagent_knowledge::files::document_name_for(&prefix, dir, f) else {
            failed.push((f.display().to_string(), "no document name".into()));
            continue;
        };
        let body = match std::fs::read_to_string(f) {
            Ok(b) => b,
            Err(e) => {
                failed.push((f.display().to_string(), format!("read failed: {e}")));
                continue;
            }
        };
        bytes += body.len();
        if dry_run {
            println!("  plan  {name}  {} B", body.len());
            continue;
        }
        let resp = client()
            .put(format!(
                "{url}/api/v1/knowledge/raw/{}",
                urlencoding::encode(&name)
            ))
            .json(&serde_json::json!({ "content": body }))
            .send();
        match resp {
            Ok(r) if r.status().is_success() => {
                let v: serde_json::Value = r.json().unwrap_or(serde_json::Value::Null);
                let n = v["chunks"].as_u64().unwrap_or(0) as usize;
                chunks += n;
                println!("  ok    {name}  {} B -> {n} chunks", body.len());
            }
            Ok(r) => failed.push((f.display().to_string(), format!("HTTP {}", r.status()))),
            Err(e) => failed.push((f.display().to_string(), format!("{e}"))),
        }
    }
    let after = if dry_run {
        before
    } else {
        documents_snapshot(url)
    };
    println!();
    println!("dir        {}", dir.display());
    println!("prefix     {prefix}");
    println!(
        "walked     {} md file(s){}",
        md.len(),
        if other > 0 {
            format!(", {other} non-md file(s) not ingested")
        } else {
            String::new()
        }
    );
    println!(
        "ingested   {} file(s){}",
        files.len() - failed.len(),
        if dry_run {
            " (dry run — nothing written)"
        } else {
            ""
        }
    );
    println!("bytes      {bytes}");
    println!("chunks     {chunks} (as reported by the ingest endpoint)");
    match (before, after) {
        (Some((db, cb)), Some((da, ca))) => {
            println!("documents  {db} -> {da}");
            println!("chunk rows {cb} -> {ca}");
        }
        _ => println!("documents  unreadable (daemon not reachable?)"),
    }
    if !failed.is_empty() {
        println!();
        for (f, why) in &failed {
            println!("  FAIL  {f}  {why}");
        }
        bail!("{} file(s) failed to ingest", failed.len());
    }
    Ok(())
}

/// (documents, chunk rows) as the daemon reports them, or `None` when the list
/// cannot be read — the caller prints that instead of a fake zero.
fn documents_snapshot(url: &str) -> Option<(usize, usize)> {
    let v: serde_json::Value = client()
        .get(format!("{url}/api/v1/knowledge/documents"))
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    let rows = v["documents"].as_array().cloned().unwrap_or_default();
    let chunks = rows
        .iter()
        .map(|r| r["chunk_count"].as_u64().unwrap_or(0) as usize)
        .sum();
    Some((rows.len(), chunks))
}

/// Non-md files under `dir`, counted so the report can say what it left out.
fn count_non_md(dir: &std::path::Path) -> usize {
    let mut n = 0usize;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|x| x.to_str()) != Some("md") {
                n += 1;
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The probe space is one list, and the doctor's recall call says it is
    /// a probe (t251/t252). If a rename splits these, this test is the only
    /// thing standing between us and orphaned junk.
    #[test]
    fn probe_space_is_self_consistent() {
        assert!(PROBE_DOC_NAME.starts_with("__probe__/"));
        assert!(!PROBE_DOC_NAME_LEGACY.starts_with("__probe__/"));
        assert!(PROBE_ENTITY_NAME.starts_with("__probe__"));
        assert!(!PROBE_ENTITY_NAME_LEGACY.starts_with("__probe__"));
        assert!(PROBE_MEMORY_CONTENT.starts_with("doctor probe"));
        assert!(PROBE_MEMORY_NAMESPACE.starts_with("agent:"));
        assert!(
            DOCTOR_RECALL_URL.contains("source=probe"),
            "a probe recall must declare itself: {DOCTOR_RECALL_URL}"
        );
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .expect("http client builds")
}

// ---------------------------------------------------------------------------
// doctor: end-to-end self-check against a running daemon
// ---------------------------------------------------------------------------

struct Check {
    name: String,
    ok: bool,
    detail: String,
}

/// Any CLI session store present on this machine?
fn self_has_cli_histories() -> bool {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_default();
    [
        home.join(".claude").join("projects"),
        home.join(".dsh").join("sessions"),
        home.join(".local/share/opencode"),
        home.join(".ruagent").join("data").join("transcripts"),
    ]
    .iter()
    .any(|p| p.is_dir())
}

fn doctor(url: &str, cleanup: bool) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let get = |path: &str| -> Option<serde_json::Value> {
        client
            .get(format!("{url}{path}"))
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .ok()
    };
    let post = |path: &str, body: &str| -> Option<serde_json::Value> {
        client
            .post(format!("{url}{path}"))
            .header("content-type", "application/json")
            .body(body.to_string())
            .send()
            .ok()?
            .error_for_status()
            .ok()?
            .json()
            .ok()
    };
    let mut checks: Vec<Check> = Vec::new();

    // 1. daemon + embedder
    match get("/api/v1/knowledge/documents") {
        Some(v) => {
            let embedder = v["embedder"].as_str().unwrap_or("?");
            checks.push(Check {
                name: "daemon + knowledge".into(),
                ok: true,
                detail: format!("embedder = {embedder}"),
            });
            let semantic = embedder != "hash-embedder";
            // Advisory when the operator explicitly pinned the offline
            // fallback (CI does: RUAGENT_EMBEDDER=hash).
            let pinned = std::env::var("RUAGENT_EMBEDDER").as_deref() == Ok("hash");
            checks.push(Check {
                name: "semantic embeddings".into(),
                ok: semantic || pinned,
                detail: if semantic {
                    "real model active".into()
                } else if pinned {
                    "hash fallback (pinned via RUAGENT_EMBEDDER=hash) — advisory".into()
                } else {
                    "hash fallback — run with network once to pull the model (hf-mirror friendly)"
                        .into()
                },
            });
        }
        None => checks.push(Check {
            name: "daemon + knowledge".into(),
            ok: false,
            detail: format!("daemon unreachable at {url} (ruagent serve)"),
        }),
    }

    // 2. semantic recall probe: distinct phrasings must hit the same doc.
    if let Some(doc) = post(
        "/api/v1/knowledge/ingest",
        &format!(
            r##"{{"name":"{PROBE_DOC_NAME}","content":"# Doctor probe\n\nThe quarterly ritual involves lighting the braziers at dawn and sounding the bronze bell twice."}}"##
        ),
    ) {
        let _ = doc; // ingest ok
        let a = get("/api/v1/knowledge/search?q=what%20happens%20at%20sunrise%20every%20quarter");
        let hit = a
            .and_then(|v| v["hits"].as_array().cloned())
            .map(|h| !h.is_empty() && h[0]["document"] == PROBE_DOC_NAME)
            .unwrap_or(false);
        checks.push(Check {
            name: "semantic recall".into(),
            ok: hit,
            detail: if hit {
                "sunrise→dawn probe hit".into()
            } else {
                "probe missed — recall quality".into()
            },
        });
    } else {
        checks.push(Check {
            name: "semantic recall".into(),
            ok: false,
            detail: "ingest failed".into(),
        });
    }

    // 3. memory lifecycle: write → recall → supersede
    let mem = post(
        "/api/v1/memory/write",
        &format!(
            r#"{{"store":"observation","namespace":"{PROBE_MEMORY_NAMESPACE}","content":"{PROBE_MEMORY_CONTENT}"}}"#
        ),
    );
    checks.push(Check {
        name: "memory write".into(),
        ok: mem.is_some(),
        detail: mem
            .map(|m| m["outcome"].as_str().unwrap_or("?").to_string())
            .unwrap_or_else(|| "failed".into()),
    });
    let recall = get(DOCTOR_RECALL_URL);
    let mem_hit = recall
        .and_then(|v| v["memories"].as_array().cloned())
        .map(|m| {
            m.iter()
                .any(|x| x["content"].as_str().unwrap_or("").contains("chrome"))
        })
        .unwrap_or(false);
    checks.push(Check {
        name: "memory recall (semantic+fts)".into(),
        ok: mem_hit,
        detail: if mem_hit {
            "kettle probe recalled".into()
        } else {
            "not recalled".into()
        },
    });

    // 4. graph: entity + fact + as-of
    if let Some(e1) = post(
        "/api/v1/graph/entity",
        &format!(r#"{{"name":"{PROBE_ENTITY_NAME}","kind":"tool"}}"#),
    ) {
        let id = e1["id"].as_i64().unwrap_or(0);
        let facts = get(&format!("/api/v1/graph/entity/{id}"));
        checks.push(Check {
            name: "graph entity".into(),
            ok: facts.is_some(),
            detail: format!("entity #{id} readable"),
        });
    } else {
        checks.push(Check {
            name: "graph entity".into(),
            ok: false,
            detail: "create failed".into(),
        });
    }

    // 5. session sync: sources present? The indexer's first pass races
    // daemon boot — poll briefly before declaring failure.
    {
        let mut sessions: Vec<serde_json::Value> = Vec::new();
        for _ in 0..12 {
            if let Some(v) = get("/api/v1/sessions?limit=500") {
                sessions = v["sessions"].as_array().cloned().unwrap_or_default();
                if !sessions.is_empty() {
                    break;
                }
            }
            std::thread::sleep(Duration::from_secs(3));
        }
        let sources: Vec<String> = {
            let mut s: Vec<String> = sessions
                .iter()
                .filter_map(|x| x["source"].as_str().map(str::to_owned))
                .collect();
            s.sort();
            s.dedup();
            s
        };
        // On a machine with no CLI histories (fresh CI runner) there is
        // legitimately nothing to sync — advisory then.
        let no_histories = !self_has_cli_histories();
        checks.push(Check {
            name: "session sync".into(),
            ok: !sessions.is_empty() || no_histories,
            detail: if sessions.is_empty() && no_histories {
                "no CLI histories on this machine (advisory)".into()
            } else {
                format!("{} sessions from: {}", sessions.len(), sources.join(", "))
            },
        });
    }

    // 6. distillation pipeline (dry signal): last distill_log entry age
    // (API has no distill_log endpoint yet — skip quietly when absent)

    // Probe hygiene (t252): the removable probe document is deleted
    // BEFORE the report, so cleanup is part of running the command and not
    // something a human has to remember.
    {
        let (clean, lines) = sweep_probe_artifacts(url, cleanup);
        checks.push(Check {
            name: "probe hygiene".into(),
            ok: clean,
            detail: lines.join("; "),
        });
    }

    // report
    println!(
        "ruagent doctor — {}
",
        url
    );
    let mut failed = 0;
    for c in &checks {
        let mark = if c.ok { "PASS" } else { "FAIL" };
        if !c.ok {
            failed += 1;
        }
        println!("  [{mark}] {:<26} {}", c.name, c.detail);
    }
    println!();
    if failed == 0 {
        println!("all checks passed");
        Ok(())
    } else {
        println!("{failed} check(s) failed");
        bail!("{failed} doctor checks failed")
    }
}

fn list_agents(url: &str) -> Result<()> {
    let resp: serde_json::Value = client()
        .get(format!("{url}/api/v1/agents"))
        .send()
        .context("daemon unreachable (is `ruagent serve` running?)")?
        .error_for_status()?
        .json()
        .context("parsing response")?;
    // Roles first, then runtimes — the two layers of design §4.1
    // (user ruling 2026-09-17: agents are roles; claude-code/dsh/opencode
    // are runtimes).
    let mut cards: Vec<&serde_json::Value> = resp["agents"]
        .as_array()
        .context("bad response")?
        .iter()
        .collect();
    cards.sort_by_key(|a| a["kind"].as_str().unwrap_or("role") != "role");
    for a in cards {
        let enabled = if a["enabled"].as_bool().unwrap_or(false) {
            "*"
        } else {
            " "
        };
        let kind = a["kind"].as_str().unwrap_or("role");
        println!(
            "{enabled} {:<10} {:<8} {:<12} {}",
            a["name"].as_str().unwrap_or("?"),
            kind,
            a["harness"].as_str().unwrap_or("?"),
            a["description"].as_str().unwrap_or("")
        );
    }
    Ok(())
}

fn status(url: &str) -> Result<()> {
    let resp: serde_json::Value = client()
        .get(format!("{url}/api/v1/tasks"))
        .send()
        .context("daemon unreachable (is `ruagent serve` running?)")?
        .error_for_status()?
        .json()
        .context("parsing response")?;
    let tasks = resp["tasks"].as_array().context("bad response")?;
    if tasks.is_empty() {
        println!("no tasks");
        return Ok(());
    }
    for t in tasks {
        let title: String = t["title"]
            .as_str()
            .unwrap_or("?")
            .chars()
            .take(48)
            .collect();
        println!(
            "{:<12} {:<48} {}",
            t["status"].as_str().unwrap_or("?"),
            title,
            t["id"].as_str().unwrap_or("?")
        );
    }
    Ok(())
}

fn run(url: &str, prompt: &str, agent: Option<&str>) -> Result<()> {
    let http = client();

    let title: String = prompt.chars().take(48).collect();
    let task: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks"))
        .json(&serde_json::json!({ "title": title, "intent": prompt }))
        .send()
        .context("creating task (daemon reachable?)")?
        .error_for_status()?
        .json()?;
    let task_id = task["id"].as_str().context("missing task id")?;

    let mut body = serde_json::json!({ "prompt": prompt });
    if let Some(a) = agent {
        body["agent"] = serde_json::json!(a);
    }
    let run: serde_json::Value = http
        .post(format!("{url}/api/v1/tasks/{task_id}/runs"))
        .json(&body)
        .send()
        .context("starting run")?
        .error_for_status()?
        .json()?;
    if run["status"] != "spawning" {
        let msg = run["error"].as_str().unwrap_or("run did not start");
        bail!("run failed to start: {msg}");
    }
    let run_id = run["id"].as_str().context("missing run id")?;
    let agent_name = agent.unwrap_or("(default)");
    println!("run {run_id} on {agent_name}\n");

    // Stream the run's events over SSE until the end marker.
    let resp = http
        .get(format!("{url}/api/v1/runs/{run_id}/events"))
        .send()
        .context("opening event stream")?
        .error_for_status()?;
    let reader = std::io::BufReader::new(resp);
    let mut current_event = String::new();
    for line in reader.lines() {
        let line = line.context("reading event stream")?;
        if let Some(name) = line.strip_prefix("event:") {
            current_event = name.trim().to_string();
        } else if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if current_event == "end" {
                let v: serde_json::Value = serde_json::from_str(data).unwrap_or_default();
                println!("\n--- finished: {}", v["status"].as_str().unwrap_or("?"));
                return Ok(());
            }
            let v: serde_json::Value = match serde_json::from_str(data) {
                Ok(v) => v,
                Err(_) => continue,
            };
            print_event(&v["event"]);
        }
    }
    bail!("event stream closed without an end marker")
}

/// Render one run event to the terminal (M1: text + usage + errors).
fn print_event(event: &serde_json::Value) {
    let kind = event["type"].as_str().unwrap_or("");
    match kind {
        "agent_message_chunk" => {
            if let Some(text) = event["content"][0]["text"].as_str() {
                print!("{text}");
                use std::io::Write as _;
                let _ = std::io::stdout().flush();
            }
        }
        "tool_call" => {
            if let Some(title) = event["title"].as_str() {
                eprintln!("\n[tool] {title}");
            }
        }
        "usage_update" => {
            let used = event["usage"]["used"].as_u64().unwrap_or(0);
            let size = event["usage"]["size"].as_u64().unwrap_or(0);
            eprintln!("[usage] {used}/{size} tokens in context");
        }
        "error" => {
            if let Some(msg) = event["message"].as_str() {
                eprintln!("[error] {msg}");
            }
        }
        "permission_requested" => {
            if let Some(title) = event["title"].as_str() {
                eprintln!(
                    "[permission] {title} — answer via: ruagent (see POST /api/v1/permissions)"
                );
            }
        }
        _ => {}
    }
}

fn skills_cmd(url: &str, do_sync: bool) -> Result<()> {
    if do_sync {
        let resp: serde_json::Value = client()
            .post(format!("{url}/api/v1/skills/sync"))
            .send()
            .context("daemon unreachable (is `ruagent serve` running?)")?
            .error_for_status()?
            .json()?;
        println!(
            "installed {} new, {} already in sync",
            resp["installed"], resp["skipped"]
        );
        return Ok(());
    }
    let resp: serde_json::Value = client()
        .get(format!("{url}/api/v1/skills"))
        .send()
        .context("daemon unreachable (is `ruagent serve` running?)")?
        .error_for_status()?
        .json()?;
    let skills = resp["skills"].as_array().context("bad response")?;
    if skills.is_empty() {
        println!("no skills (add them to ~/.ruagent/skills/ or .ruagent/skills/)");
        return Ok(());
    }
    for s in skills {
        println!(
            "{:<8} {:<20} {}",
            s["source"].as_str().unwrap_or("?"),
            s["name"].as_str().unwrap_or("?"),
            s["description"].as_str().unwrap_or("")
        );
    }
    Ok(())
}
