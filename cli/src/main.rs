//! ruagent CLI: thin client over the daemon's local API (design §3).

use std::io::BufRead;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "ruagent",
    version,
    about = "Local-first agent engineering platform"
)]
struct Cli {
    /// Base URL of a running daemon.
    ///
    /// GLOBAL ON PURPOSE. Written the way a caller naturally writes it -- a flag
    /// on the subcommand it belongs to, `ruagent doctor --url <url>` -- this used
    /// to fail with `error: unexpected argument '--url' found` (exit 2), because
    /// clap only accepted it in FRONT of the subcommand. That killed the CI
    /// Doctor step, and Panel serves / Panel e2e were skipped with it. With
    /// `global = true` the flag parses on either side of the subcommand and the
    /// value reaches `cli.url` in both orders.
    #[arg(
        long,
        global = true,
        env = "RUAGENT_URL",
        default_value = "http://127.0.0.1:8787"
    )]
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
        /// Bind a NON-loopback address on purpose (t76 / audit A-3). Without this
        /// (or RUAGENT_ALLOW_REMOTE=1) the daemon refuses to start on anything but
        /// loopback: it has no authentication, so a remote bind publishes every
        /// read and write endpoint to the network.
        #[arg(long)]
        allow_remote: bool,
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
    /// Distil a transcript into memories + graph facts.
    ///
    /// WHEN: after a long session (one distilled turn is cheaper than re-reading
    /// it every time), or to close the backlog of sessions no successful pass was
    /// ever recorded for -- `--unprocessed N` is that backlog pass.
    ///
    /// COST: a real pass spends model calls and writes memory/graph rows. One
    /// session is NOT necessarily one call: a transcript over the plan's byte
    /// budget is cut into windows and EVERY window is its own call, so a long
    /// session costs several. Nothing runs for free here, which is why
    /// `--dry-run` exists: with `--unprocessed` it prices the backlog WITHOUT
    /// calling a model and WITHOUT writing anything. (With `--session` the route
    /// still extracts once -- that is the endpoint's documented meaning of a dry
    /// run: it tells you what WOULD be written -- and only then writes nothing.)
    ///
    /// EXIT CODE: 0 when nothing failed, non-zero when any attempt failed, so a
    /// script can tell a pass that worked from one that did not.
    #[command(group(
        ArgGroup::new("target")
            .required(true)
            .args(["session", "unprocessed"])
    ))]
    Distill {
        /// ONE session key, exactly as `ruagent status` / `/api/v1/sessions`
        /// reports it.
        #[arg(long)]
        session: Option<String>,
        /// Distil up to N sessions the ledger has no successful attempt for,
        /// newest first. The daemon clamps this to its own per-pass ceiling and
        /// the report prints how many of the undistilled total were planned.
        #[arg(long)]
        unprocessed: Option<u32>,
        /// WHICH extractor runs, in the DAEMON'S OWN vocabulary: `rules` (free --
        /// no model call at all), `acp` (one chat turn per window: the default and
        /// the only implementation that costs money) or `both`. The CLI keeps no
        /// second copy of that list: an unknown value is refused by the daemon,
        /// which names the accepted ones.
        #[arg(long)]
        extractor: Option<String>,
        /// Plan and price only; write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Print the daemon's report as JSON (same shape for one session and for
        /// the backlog) instead of the human summary.
        #[arg(long)]
        json: bool,
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
        Cmd::Serve {
            addr,
            root,
            allow_remote,
        } => {
            let addr: std::net::SocketAddr = addr.parse().context("invalid listen address")?;
            let root = root.unwrap_or_else(ruagent_daemon::default_root);
            // Only serve needs a runtime; the other subcommands are sync
            // (reqwest::blocking's internal runtime must never be dropped
            // inside an async context).
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .context("building tokio runtime")?;
            runtime.block_on(ruagent_daemon::serve_with_remote(root, addr, allow_remote))
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
        Cmd::Distill {
            session,
            unprocessed,
            extractor,
            dry_run,
            json,
        } => distill_cmd(
            &cli.url,
            session.as_deref(),
            unprocessed,
            extractor.as_deref(),
            dry_run,
            json,
        ),
        Cmd::Knowledge { cmd } => knowledge_cmd(&cli.url, cmd),
    }
}

// ---------------------------------------------------------------------------
// wiki: compile sources into interlinked pages (thin client over the API)
// ---------------------------------------------------------------------------

/// Response -> JSON with the daemon's {message} error body surfaced.
///
/// THE UNPARSEABLE-SUCCESS CASE CARRIES ITS LIKELY CAUSE (t9 / issue #46): a daemon
/// built from an OLDER tree answers a path it does not serve with the PANEL's
/// `index.html` and a 200, so `expected value at line 1 column 1` is what a stale
/// `ruagent serve` looks like from here -- a message that names no action for the
/// one thing the reader has to do (MEASURED against the live daemon at
/// 127.0.0.1:8787, which serves `/api/v1/distill/log` as HTML). The hint names the
/// action, and the content type plus the first bytes make the shape visible.
fn api_json(resp: reqwest::blocking::Response) -> Result<serde_json::Value> {
    let status = resp.status();
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("(no content-type)")
        .to_string();
    let text = resp.text().context("reading response")?;
    if !status.is_success() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(m) = v["message"].as_str()
        {
            bail!("{m}");
        }
        bail!("HTTP {status}: {text}");
    }
    serde_json::from_str(&text).with_context(|| {
        format!(
            "the daemon answered {status} {content_type} where JSON was expected — is the running daemon an OLDER build than this CLI? restart it (`ruagent serve`); it starts with: {}",
            truncate_chars(text.trim(), 60)
        )
    })
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
/// The probe space is a PREFIX, not a name table (t295): another task's probe
/// (`agent:__probe__t263`, `__probe__t263-node`, `__probe__/t263-x`) lives in the
/// same space and must be swept by the same rule. The names below are what
/// THIS command writes; the sweeps match on the prefixes.
const PROBE_PREFIX: &str = "__probe__";
const PROBE_MEMORY_NAMESPACE_PREFIX: &str = "agent:__probe__";
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
                if !name.starts_with(PROBE_PREFIX) && !doc_names.contains(&name.as_str()) {
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
    // ...and everything else in the same PREFIX space, whoever wrote it: the
    // probe space is a prefix, not a name table (t295). /graph/entities returns
    // [entity, edge_count] pairs, so the entity object is row[0].
    match get_json(url, "/api/v1/graph/entities?limit=500") {
        Ok(v) => {
            let rows = v["entities"].as_array().cloned().unwrap_or_default();
            if rows.len() >= 500 {
                left += 1;
                lines.push(
                    "entity listing hit its page bound (500) — some probe entities may be missed"
                        .into(),
                );
            }
            for row in rows {
                let e = row.get(0).cloned().unwrap_or(serde_json::Value::Null);
                let name = e["name"].as_str().unwrap_or_default().to_string();
                if !name.starts_with(PROBE_PREFIX) {
                    continue;
                }
                if let Some(id) = e["id"].as_i64() {
                    entity_targets.push((id, name));
                }
            }
        }
        Err(e) => {
            left += 1;
            lines.push(format!("could not list entities to sweep: {e}"));
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
    // The probe NAMESPACE is a prefix too (t295). memory/list's namespace
    // parameter matches EXACTLY, so list the space and filter here; a
    // truncated listing is reported, never treated as "nothing to sweep".
    match get_json(url, "/api/v1/memory/list?limit=5000") {
        Ok(v) => {
            let rows = v["memories"].as_array().cloned().unwrap_or_default();
            let matched = v["matched"].as_u64().unwrap_or(rows.len() as u64);
            if matched > rows.len() as u64 {
                left += 1;
                lines.push(format!(
                    "memory listing truncated: {} of {matched} rows — not every probe memory could be swept",
                    rows.len()
                ));
            }
            for row in rows {
                let ns = row["namespace"].as_str().unwrap_or_default().to_string();
                if !ns.starts_with(PROBE_MEMORY_NAMESPACE_PREFIX) {
                    continue;
                }
                if let Some(id) = row["id"].as_i64() {
                    memory_targets.push((
                        id,
                        format!("namespace prefix {PROBE_MEMORY_NAMESPACE_PREFIX} ({ns})"),
                    ));
                }
            }
        }
        Err(e) => {
            left += 1;
            lines.push(format!("could not list memories to sweep: {e}"));
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
// t45 (INT): `cli/` had no owner until this unit, and this module sits in the
// middle of the file -- 9 functions (client/doctor/list_agents/status/run/...)
// are defined AFTER it, which is what `items_after_test_module` reports.
//
// WHY AN ALLOW AND NOT A MOVE: moving ~500 lines of CLI code to the end of
// `main.rs` is a reordering of a file no one owns, in a unit whose contract is
// "make the workspace lint gate green"; a mechanical move would be a far larger
// diff to review than the lint it silences, and it would churn every line
// number in the file. The allow is scoped to the lint, on the module it
// describes, with this comment as the reason. (If `cli/` gets an owner, the
// move is the better fix -- and this attribute is the marker for it.)
#[allow(clippy::items_after_test_module)]
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
        // The probe space is a prefix, and the exact names live inside it
        // (t295): a rename that leaves the space is the bug this pins.
        assert!(PROBE_DOC_NAME.starts_with(PROBE_PREFIX));
        assert!(PROBE_ENTITY_NAME.starts_with(PROBE_PREFIX));
        assert!(PROBE_MEMORY_NAMESPACE.starts_with(PROBE_MEMORY_NAMESPACE_PREFIX));
        // The legacy names are deliberately NOT in the prefix space: they are
        // only swept by --cleanup.
        assert!(!PROBE_DOC_NAME_LEGACY.starts_with(PROBE_PREFIX));
        assert!(!PROBE_ENTITY_NAME_LEGACY.starts_with(PROBE_PREFIX));
    }

    /// t295: a sweep that cannot even LIST must fail loudly. Pointed at a dead
    /// port the listing fails, so the hygiene check must come back not-clean
    /// and say why — the alternative (a clean-looking no-op) is the failure
    /// mode this report line exists to prevent.
    #[test]
    fn sweep_reports_a_listing_that_cannot_be_read() {
        let (clean, lines) = sweep_probe_artifacts("http://127.0.0.1:1", false);
        let joined = lines.join("; ");
        assert!(
            !clean,
            "a dead daemon must not read as a clean sweep: {joined}"
        );
        assert!(
            joined.contains("could not list"),
            "the reason must be printed, not skipped: {joined}"
        );
        assert!(PROBE_MEMORY_NAMESPACE.starts_with("agent:"));
        assert!(
            DOCTOR_RECALL_URL.contains("source=probe"),
            "a probe recall must declare itself: {DOCTOR_RECALL_URL}"
        );
    }

    /// t9 / issue #46: the distillation check must not PASS because it could not
    /// ask. Without an answer it says 未检查 and is NOT ok, so `doctor` reports it
    /// instead of omitting the line -- and the same reading is reached through a
    /// real socket that refuses, not only by handing the function a None.
    #[test]
    fn doctor_distillation_check_says_not_checked_when_nobody_answers() {
        let refused = get_json("http://127.0.0.1:1", "/api/v1/distill/log?limit=1");
        assert!(
            refused.is_err(),
            "a dead port must not answer a ledger read"
        );
        // No daemon at all: the line names THAT cause.
        let c = distill_check(false, refused);
        assert!(
            !c.ok,
            "an unanswered check must not read as PASS: {}",
            c.detail
        );
        assert!(c.detail.contains("未检查"), "{}", c.detail);
        assert!(c.detail.contains("daemon 不可达"), "{}", c.detail);

        // A daemon that IS up but does not serve this endpoint is a DIFFERENT
        // finding: an older build answers the path with the panel's HTML, and
        // "daemon 不可达" would send the reader to the wrong machine.
        let c = distill_check(true, Err("error decoding response body".into()));
        assert!(!c.ok, "not answering is not a PASS either: {}", c.detail);
        assert!(c.detail.contains("未检查"), "{}", c.detail);
        assert!(!c.detail.contains("daemon 不可达"), "{}", c.detail);
        assert!(
            c.detail.contains("error decoding response body"),
            "the cause must be readable: {}",
            c.detail
        );
    }

    /// t9 / #46: doctor's reading of a REACHABLE ledger -- the newest attempt and
    /// the cumulative usage -- and the 0028 rule that a NULL column is not a zero.
    #[test]
    fn doctor_distillation_reading_keeps_nulls_out_of_the_numbers() {
        let legacy = serde_json::json!({
            "entries": [{
                "id": 41,
                "session_key": "k",
                "distilled_at": "2026-01-02T03:04:05Z",
                "status": serde_json::Value::Null,
                "failure_reason": serde_json::Value::Null,
            }],
            "total": 33,
            "totals": {
                "tokens_used": serde_json::Value::Null,
                "cost_usd": serde_json::Value::Null,
                "tokens_used_not_recorded": 33,
                "cost_usd_not_recorded": 33,
            },
        });
        let line = distill_reading(&legacy);
        assert!(line.contains("2026-01-02T03:04:05Z"), "{line}");
        assert!(
            line.contains("[not_recorded]"),
            "a NULL status is the ledger's own word for it: {line}"
        );
        assert!(
            line.contains("not recorded (33 of 33 attempt(s))"),
            "{line}"
        );
        assert!(
            !line.contains("tokens 0") && !line.contains("$0.0000"),
            "a NULL is not a measurement: {line}"
        );

        let measured = serde_json::json!({
            "entries": [{
                "id": 7,
                "distilled_at": "2026-02-02T00:00:00Z",
                "status": "failed",
                "failure_reason": "no content to distil: the session has no user messages",
            }],
            "total": 4,
            "totals": {
                "tokens_used": 1240,
                "cost_usd": 0.0012,
                "tokens_used_not_recorded": 1,
                "cost_usd_not_recorded": 1,
            },
        });
        let line = distill_reading(&measured);
        assert!(line.contains("[failed]"), "{line}");
        assert!(
            line.contains("no content to distil"),
            "the failure reason must be readable: {line}"
        );
        assert!(line.contains("tokens 1240"), "{line}");
        assert!(line.contains("$0.0012"), "{line}");

        // An empty ledger is a REACHABLE ledger with nothing in it: ok, and said.
        let empty = serde_json::json!({
            "entries": [],
            "total": 0,
            "totals": {"tokens_used": serde_json::Value::Null, "cost_usd": serde_json::Value::Null,
                       "tokens_used_not_recorded": 0, "cost_usd_not_recorded": 0},
        });
        let line = distill_reading(&empty);
        assert!(line.contains("no attempts recorded yet"), "{line}");
    }

    /// t12 / CI: `--url` is GLOBAL, so the flag parses on EITHER side of the
    /// subcommand. Before this, only `ruagent --url <url> doctor` parsed; the
    /// natural order -- `ruagent doctor --url <url>`, which is how the CI step
    /// is written -- died with `error: unexpected argument '--url' found`
    /// (exit 2), failing the Doctor step and skipping Panel serves and Panel E2E
    /// with it.
    ///
    /// Both orders are asserted HERE, on the parsed struct, because the CI shape is
    /// a COMMAND LINE, and a command line is only known to work once it has been
    /// parsed: "the field exists" and "this command line is accepted" are two
    /// different readings (the gen3-backlog C81 lesson).
    #[test]
    fn the_url_flag_parses_on_either_side_of_the_subcommand() {
        // The natural order: the flag AFTER the subcommand, on the subcommand's
        // own line, which is how a caller writes it.
        let after = Cli::try_parse_from(["ruagent", "doctor", "--url", "http://127.0.0.1:8891"])
            .expect("`doctor --url <url>` must parse: the flag is global");
        assert_eq!(
            after.url, "http://127.0.0.1:8891",
            "the value reaches cli.url"
        );
        assert!(
            matches!(after.cmd, Cmd::Doctor { cleanup: false }),
            "and the subcommand is still the one that was asked for"
        );

        // The FRONT-LOADED order: the one that already worked, and must keep
        // working.
        let before = Cli::try_parse_from(["ruagent", "--url", "http://127.0.0.1:8891", "doctor"])
            .expect("`--url <url> doctor` must keep parsing");
        assert_eq!(before.url, "http://127.0.0.1:8891");
        assert!(matches!(before.cmd, Cmd::Doctor { cleanup: false }));

        // A subcommand with arguments of its own takes the global too, so the
        // flag is global rather than merely accepted after `doctor`.
        let nested = Cli::try_parse_from([
            "ruagent",
            "distill",
            "--session",
            "sess/key",
            "--url",
            "http://127.0.0.1:9",
        ])
        .expect("a nested subcommand takes the global as well");
        assert_eq!(nested.url, "http://127.0.0.1:9");

        // The default is untouched: the local daemon when no flag is given.
        // (Skipped when RUAGENT_URL is set: the env fallback is deliberate.)
        if std::env::var("RUAGENT_URL").is_err() {
            let default = Cli::try_parse_from(["ruagent", "status"]).expect("status parses");
            assert_eq!(default.url, "http://127.0.0.1:8787");
        }
    }

    /// t9 / #47: the CLI surface itself, without a daemon. Exactly one target is
    /// required and the two are exclusive -- `--session <key>` and
    /// `--unprocessed <n> --dry-run` are the two calls the contract names.
    #[test]
    fn distill_targets_are_exclusive_and_one_is_required() {
        let one = Cli::try_parse_from(["ruagent", "distill", "--session", "sess/key"])
            .expect("--session parses");
        match one.cmd {
            Cmd::Distill {
                session,
                unprocessed,
                extractor,
                dry_run,
                json,
            } => {
                assert_eq!(session.as_deref(), Some("sess/key"));
                assert!(unprocessed.is_none());
                // Absent means "the daemon's default" (acp) rather than a value
                // the CLI picked, so it must reach the request as absent.
                assert!(extractor.is_none());
                assert!(!dry_run && !json);
            }
            _ => panic!("distill must parse as Cmd::Distill"),
        }

        let backlog = Cli::try_parse_from([
            "ruagent",
            "distill",
            "--unprocessed",
            "5",
            "--extractor",
            "rules",
            "--dry-run",
            "--json",
        ])
        .expect("--unprocessed --extractor --dry-run --json parses");
        match backlog.cmd {
            Cmd::Distill {
                session,
                unprocessed,
                extractor,
                dry_run,
                json,
            } => {
                assert_eq!(unprocessed, Some(5));
                assert_eq!(extractor.as_deref(), Some("rules"));
                assert!(dry_run && json);
                assert!(session.is_none());
            }
            _ => panic!("distill must parse as Cmd::Distill"),
        }

        assert!(
            Cli::try_parse_from(["ruagent", "distill"]).is_err(),
            "a target is required: the CLI must not guess which pass to run"
        );
        assert!(
            Cli::try_parse_from(["ruagent", "distill", "--session", "k", "--unprocessed", "2"])
                .is_err(),
            "one session and a backlog pass are different jobs and cannot be one call"
        );
    }

    /// t9 / #46: the ledger decides what became of an attempt, and an attempt that
    /// wrote nothing is never reported as `ok`.
    #[test]
    fn classify_prefers_the_ledger_row_and_never_invents_ok() {
        let row = serde_json::json!({
            "status": "empty",
            "failure_reason": serde_json::Value::Null,
        });
        assert_eq!(classify_distill_attempt(Some(&row), None).0, "empty");
        // The row wins even when the call ALSO errored -- the ledger is the
        // receipt -- but the error is then the reason, so the cause is not lost.
        let (status, reason) = classify_distill_attempt(Some(&row), Some("boom"));
        assert_eq!(status, "empty");
        assert_eq!(reason.as_deref(), Some("boom"));

        // Ok without a row is NOT ok: the ledger holds no confirmation of it.
        let (status, reason) = classify_distill_attempt(None, None);
        assert_eq!(status, DISTILL_STATUS_NOT_RECORDED);
        assert!(
            reason
                .unwrap()
                .contains("without writing a distill_log row"),
            "the fourth state must say what it means"
        );

        // Err without a row is `failed`, with the daemon's own text (t4 writes the
        // refusal text; it carries no SQL vocabulary).
        let (status, reason) = classify_distill_attempt(None, Some("no content to distil"));
        assert_eq!(status, "failed");
        assert_eq!(reason.as_deref(), Some("no content to distil"));

        // A row whose status is NULL is the ledger's fourth state, not `ok`.
        let null_row = serde_json::json!({"status": serde_json::Value::Null});
        assert_eq!(
            classify_distill_attempt(Some(&null_row), None).0,
            DISTILL_STATUS_NOT_RECORDED
        );
    }

    /// t9 / #46: the failure rule, in ONE place -- a pass that failed any attempt
    /// must fail the command, and a pass that recorded none must not.
    #[test]
    fn a_failed_attempt_fails_the_command_and_a_clean_pass_does_not() {
        let clean = serde_json::json!({
            "dry_run": false, "attempted": 2, "ok": 1, "empty": 1, "failed": 0, "results": [],
        });
        assert!(
            report_distill(&clean, false, vec![]).is_ok(),
            "empty is an OUTCOME, not a failure: a backlog pass that found nothing to write must not fail the command"
        );
        let failed = serde_json::json!({
            "dry_run": false, "attempted": 2, "ok": 0, "empty": 0, "failed": 2, "results": [],
        });
        let err = report_distill(&failed, false, vec![])
            .expect_err("a failed attempt must make the command exit non-zero")
            .to_string();
        assert!(err.contains("2 of 2"), "{err}");
        // A DRY RUN that planned fine is a success (nothing was attempted, so
        // nothing failed), which is what makes `--unprocessed N --dry-run` usable
        // in a script that checks the exit code.
        let dry = serde_json::json!({
            "dry_run": true, "attempted": 0, "ok": 0, "empty": 0, "failed": 0, "results": [],
        });
        assert!(report_distill(&dry, false, vec![]).is_ok());
    }

    /// t9: a ledger that cannot be read is an ERROR, not an empty ledger. The
    /// distinction is the whole point: "this attempt recorded nothing" is a finding
    /// about distillation, while "I could not ask" means nothing was checked.
    #[test]
    fn a_ledger_that_cannot_be_read_is_an_error_not_an_empty_one() {
        assert!(
            ledger_rows("http://127.0.0.1:1", "any/session", 5).is_err(),
            "a dead daemon must not read as 'this session has no ledger rows'"
        );
        assert!(ledger_row_since("http://127.0.0.1:1", "any/session", 0).is_err());
    }

    /// t9 / #47: the backlog report prints the daemon's OWN counts (no second
    /// classification), keeps the three states and the fourth apart, and never
    /// prints an unrecorded sum as 0.
    #[test]
    fn batch_report_prints_three_states_and_keeps_nulls() {
        let resp = serde_json::json!({
            "dry_run": false,
            "extractor": "acp",
            "planned": 2,
            "undistilled_total": 5,
            "attempted": 2,
            "ok": 1,
            "empty": 0,
            "failed": 1,
            "estimated": {
                "sessions": 2,
                "bytes": 4096,
                "windows": 2,
                "budget_bytes": 1048576,
                "tokens_used": serde_json::Value::Null,
                "cost_usd": serde_json::Value::Null,
                "basis": {"ok_attempts": 0, "windows_recorded": 0, "tokens_recorded": 0, "cost_recorded": 0},
            },
            "actual": {
                "rows": 2,
                "windows": serde_json::Value::Null,
                "tokens_used": serde_json::Value::Null,
                "cost_usd": serde_json::Value::Null,
                "windows_not_recorded": 2,
                "tokens_used_not_recorded": 2,
                "cost_usd_not_recorded": 2,
            },
            "results": [
                {"session_key": "a", "status": "ok", "reason": serde_json::Value::Null},
                {"session_key": "b", "status": "failed", "reason": "no content to distil"},
            ],
        });
        let joined = batch_report_lines(&resp).join("\n");
        assert!(
            joined.contains("three-state: ok=1 empty=0 failed=1 of 2 attempted (not_recorded=0)"),
            "{joined}"
        );
        assert!(
            joined.contains("[failed") && joined.contains("no content to distil"),
            "every attempt's status and reason must be in the report: {joined}"
        );
        assert!(
            joined.contains("not recorded by any of 0 successful attempt(s)"),
            "the estimate says WHY it cannot price the pass: {joined}"
        );
        assert!(
            !joined.contains("tokens 0") && !joined.contains("$0.0000"),
            "an unrecorded sum is not 0: {joined}"
        );
        assert!(
            joined.contains("3 more undistilled session(s)"),
            "the pass's own limit must not hide the rest of the backlog: {joined}"
        );
        assert!(joined.contains("actual: 2 ledger row(s)"), "{joined}");
    }

    /// t9 / #47: a dry-run pass writes nothing, so it reports no `actual` row and no
    /// per-attempt lines -- and says the model was never called.
    #[test]
    fn batch_dry_run_reports_a_plan_and_no_actuals() {
        let resp = serde_json::json!({
            "dry_run": true,
            "extractor": "acp",
            "planned": 1,
            "undistilled_total": 1,
            "attempted": 0,
            "ok": 0,
            "empty": 0,
            "failed": 0,
            "estimated": {
                "sessions": 1,
                "bytes": 2048,
                "windows": 1,
                "budget_bytes": 1048576,
                "tokens_used": 1200.0,
                "cost_usd": 0.003,
                "basis": {"ok_attempts": 12, "windows_recorded": 12, "tokens_recorded": 12, "cost_recorded": 12},
            },
            "actual": serde_json::Value::Null,
            "results": [],
        });
        let joined = batch_report_lines(&resp).join("\n");
        assert!(joined.contains("DRY RUN"), "{joined}");
        assert!(
            joined.contains("three-state: ok=0 empty=0 failed=0 of 0 attempted"),
            "{joined}"
        );
        assert!(
            !joined.contains("actual:"),
            "a dry run wrote nothing, so it has no actuals: {joined}"
        );
        assert!(joined.contains("tokens 1200"), "{joined}");
        assert!(joined.contains("$0.0030"), "{joined}");
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

/// Doctor's distillation check (t9 / issue #46), from a ledger reading; `None`
/// means the daemon did not answer.
///
/// WHY "NOT CHECKED" IS A FAILING CHECK: this command's whole output is a
/// PASS/FAIL report, and a line that reports PASS because it could not ask is the
/// one reading an operator cannot act on. The detail names the cause, so a red line
/// here is a daemon problem and not a distillation one.
fn distill_check(daemon_up: bool, reading: Result<serde_json::Value, String>) -> Check {
    match (daemon_up, reading) {
        (_, Ok(v)) => Check {
            name: "distillation pipeline".into(),
            ok: true,
            detail: distill_reading(&v),
        },
        // NO DAEMON AT ALL: the cause is the daemon, and the line says so.
        (false, Err(_)) => Check {
            name: "distillation pipeline".into(),
            ok: false,
            detail: "未检查（daemon 不可达）— start it with `ruagent serve`".into(),
        },
        // A DAEMON THAT DOES NOT SERVE THIS ENDPOINT: NOT the same finding as an
        // absent daemon. An older build answers the path with the panel's HTML (or
        // a 404), and reporting "unreachable" there would send the reader to the
        // wrong machine; the error text names what actually came back.
        (true, Err(e)) => Check {
            name: "distillation pipeline".into(),
            ok: false,
            detail: format!("未检查（endpoint failed: {e}）"),
        },
    }
}

/// The ledger's summary line: the newest attempt and the cumulative usage.
///
/// THE NULLS ARE THE POINT (0020/0028): an attempt nobody recorded a status for
/// reads `not_recorded` -- the ledger read's own word, not a fourth outcome -- and
/// a column nobody recorded reads "not recorded" WITH the count of rows that did not
/// record it. Printing 0 for either would turn "we did not measure this" into a
/// measurement, which is the mistake those columns were added to prevent.
fn distill_reading(v: &serde_json::Value) -> String {
    let total = v["total"].as_i64().unwrap_or(0);
    let totals = &v["totals"];
    let tokens = reading_int(
        &totals["tokens_used"],
        format!(
            "not recorded ({} of {total} attempt(s))",
            totals["tokens_used_not_recorded"].as_i64().unwrap_or(0)
        ),
    );
    let cost = reading_cost(
        &totals["cost_usd"],
        format!(
            "not recorded ({} of {total} attempt(s))",
            totals["cost_usd_not_recorded"].as_i64().unwrap_or(0)
        ),
    );
    let Some(entry) = v["entries"].as_array().and_then(|e| e.first()) else {
        return format!(
            "reachable, no attempts recorded yet (0 row(s)); cumulative tokens {tokens}, cost {cost}"
        );
    };
    let status = entry["status"]
        .as_str()
        .unwrap_or(DISTILL_STATUS_NOT_RECORDED);
    let reason = entry["failure_reason"]
        .as_str()
        .filter(|r| !r.trim().is_empty())
        .map(|r| format!(" — {}", truncate_chars(r, 120)))
        .unwrap_or_default();
    format!(
        "last attempt {} [{status}]{reason} (of {total} recorded); cumulative tokens {tokens}, \
         cost {cost}",
        entry["distilled_at"]
            .as_str()
            .unwrap_or("at an unrecorded time"),
    )
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

    // 1. daemon + embedder. `daemon_up` is kept because a LATER check has to tell
    // "the daemon is not running" apart from "the daemon is running but this
    // endpoint did not answer" -- two different findings with two different fixes
    // (t9's distillation check is the reader).
    let mut daemon_up = false;
    match get("/api/v1/knowledge/documents") {
        Some(v) => {
            daemon_up = true;
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

    // 6. distillation pipeline (t9 / issue #46): the LEDGER is read, not guessed
    // at. `/api/v1/distill/log` exists (t6), so this reports the LAST ATTEMPT
    // (when, what status, and why it failed) and the CUMULATIVE tokens/cost. A
    // daemon that does not answer is reported as NOT CHECKED -- the pre-t9 code
    // printed this line's absence by saying nothing, which is the silent pass the
    // issue is about ("skip quietly when absent").
    checks.push(distill_check(
        daemon_up,
        get_json(url, "/api/v1/distill/log?limit=1"),
    ));

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

// ---------------------------------------------------------------------------
// distill: the CLI's face on the two distillation routes (issues #46/#47)
// ---------------------------------------------------------------------------

/// The ledger's FOURTH state: a NULL `distill_log.status` means nobody recorded
/// what became of the attempt. Spelled once, because three surfaces read it (the
/// per-attempt table, the JSON envelope and the exit code) and a second literal
/// would let them disagree about what "the ledger says nothing" is called.
const DISTILL_STATUS_NOT_RECORDED: &str = "not_recorded";

/// A RECORDED number from the ledger, or `absent` when the column is NULL.
///
/// THE 0028 RULE REACHES THE CLI HERE: `tokens_used`, `cost_usd`, `windows`,
/// `retries` and `truncated` are NULL when nobody measured them (a rules-only
/// pass reports no usage at all; every pre-0028 row predates the columns), so a
/// printed `0` would turn "we did not record this" into "this was zero" -- the
/// one reading those columns were added to make impossible.
fn reading_int(value: &serde_json::Value, absent: String) -> String {
    value.as_i64().map(|n| n.to_string()).unwrap_or(absent)
}

/// The money half of the same rule: a sum with nothing to add is not `$0.0000`.
fn reading_cost(value: &serde_json::Value, absent: String) -> String {
    value.as_f64().map(|c| format!("${c:.4}")).unwrap_or(absent)
}

/// The same rule for an EXTRAPOLATED number.
///
/// WHY THIS IS NOT `reading_int`: the batch estimate is a mean multiplied by a
/// count (`crates/daemon/src/api.rs`, `estimate_batch`), so the daemon sends a
/// FLOAT even when the value happens to be whole (`as_i64()` would reject
/// `1200.0` and report a price that exists as unrecorded). The fraction is kept
/// when there is one -- rounding someone's bill to an integer is a number the
/// daemon never computed.
fn reading_number(value: &serde_json::Value, absent: String) -> String {
    match value.as_f64() {
        Some(n) if n.fract() == 0.0 => format!("{}", n as i64),
        Some(n) => format!("{n:.1}"),
        None => absent,
    }
}

/// The same rule for a tri-state flag: `null` = never measured, and it must not
/// print as "no" (that is a measurement nobody took).
fn reading_bool(value: &serde_json::Value, absent: String) -> String {
    match value.as_bool() {
        Some(true) => "yes".to_string(),
        Some(false) => "no".to_string(),
        None => absent,
    }
}

/// Doctor's report is one line per check: a failure reason is a sentence, not a
/// transcript.
fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut t: String = s.chars().take(max).collect();
    t.push('…');
    t
}

/// THE THREE-STATE COUNTS, spelled once so the two routes' reports cannot drift:
/// `ok`/`empty`/`failed` are the ledger's outcomes and `not_recorded` is the
/// fourth state the ledger read names (an attempt that wrote no row) -- it is NOT
/// counted as one of the three, and the line prints it so the fourth is visible
/// rather than folded away.
fn three_state_line(attempted: u32, ok: u32, empty: u32, failed: u32, not_recorded: u32) -> String {
    format!(
        "  three-state: ok={ok} empty={empty} failed={failed} of {attempted} attempted \
         (not_recorded={not_recorded})"
    )
}

/// How many entries the response itself called `status`.
///
/// A COUNT OF THE WORDS THE DAEMON USED, not a re-classification: the daemon owns
/// the vocabulary (its batch route classifies each attempt with the ledger in
/// hand), and a client that re-derived the states could print a total its own rows
/// do not add up to.
fn count_status(results: &[serde_json::Value], status: &str) -> u32 {
    results
        .iter()
        .filter(|r| r["status"].as_str() == Some(status))
        .count() as u32
}

/// WHAT ONE ATTEMPT ENDED AS, in the ledger's own vocabulary.
///
/// THE LEDGER DECIDES WHENEVER THERE IS A ROW: its `status` is what happened and
/// its `failure_reason` is why (t4 writes one row per non-dry attempt, 0024).
/// The CLI speaks for itself only when the attempt recorded nothing, and then it
/// keeps the two cases apart: an attempt that returned Ok without a row is
/// `not_recorded` -- never `ok`, because calling it `ok` would invent a
/// confirmation the ledger does not have -- while an attempt that ERRORED is
/// `failed` with the daemon's own text as the reason.
///
/// WHY THIS RULE IS WRITTEN TWICE in this repository: the daemon's batch route
/// applies it per attempt (`classify_attempt`, crates/daemon/src/api.rs) and the
/// single route reports no status field at all, so a client asking the same
/// question about one session has no way to call the server's private function.
/// The vocabulary on both sides is the ledger's.
fn classify_distill_attempt(
    row: Option<&serde_json::Value>,
    error: Option<&str>,
) -> (String, Option<String>) {
    if let Some(row) = row {
        let status = row["status"]
            .as_str()
            .unwrap_or(DISTILL_STATUS_NOT_RECORDED)
            .to_string();
        // A row that says why keeps its own reason; when it recorded none, the
        // error the call returned is the only cause anyone has.
        let reason = row["failure_reason"]
            .as_str()
            .filter(|r| !r.trim().is_empty())
            .map(str::to_owned)
            .or_else(|| error.map(str::to_owned));
        return (status, reason);
    }
    match error {
        Some(e) => ("failed".to_string(), Some(e.to_string())),
        None => (
            DISTILL_STATUS_NOT_RECORDED.to_string(),
            Some("the attempt returned without writing a distill_log row".to_string()),
        ),
    }
}

/// ONE attempt's reading, from its LEDGER ROW -- the receipt a later reader
/// queries -- rather than from the response body's echo of it.
fn ledger_attempt_lines(row: &serde_json::Value) -> Vec<String> {
    vec![
        format!(
            "  ledger #{} at {} — agent {}",
            row["id"].as_i64().unwrap_or(0),
            row["distilled_at"].as_str().unwrap_or("an unrecorded time"),
            row["agent"].as_str().unwrap_or("none (rules only)"),
        ),
        format!(
            "  wrote: {} memories, {} entities, {} relations (already stated {}, refused {})",
            row["memories_written"].as_i64().unwrap_or(0),
            row["entities_written"].as_i64().unwrap_or(0),
            row["relations_written"].as_i64().unwrap_or(0),
            reading_int(&row["relations_duplicate"], "not reported".into()),
            reading_int(&row["relations_refused"], "not reported".into()),
        ),
        format!(
            "  usage: tokens {} (context {}), cost {}, windows {}, truncated {}, retries {}",
            reading_int(&row["tokens_used"], "not recorded".into()),
            reading_int(&row["context_size"], "not recorded".into()),
            reading_cost(&row["cost_usd"], "not recorded".into()),
            reading_int(&row["windows"], "not recorded".into()),
            reading_bool(&row["truncated"], "not recorded".into()),
            reading_int(&row["retries"], "not recorded".into()),
        ),
    ]
}

/// ONE attempt's own report, from the distillation response (`distilled`) -- used
/// when there is no ledger row to read, which is a dry run's normal case (it
/// writes no row by design).
fn outcome_lines(outcome: &serde_json::Value, verb: &str) -> Vec<String> {
    let mut out = vec![
        format!(
            "  {verb}: {} memories ({} already known), {} entities, {} relations \
             (already stated {}, refused {})",
            outcome["memories_written"].as_u64().unwrap_or(0),
            outcome["memories_skipped"].as_u64().unwrap_or(0),
            outcome["entities_written"].as_u64().unwrap_or(0),
            outcome["relations_written"].as_u64().unwrap_or(0),
            reading_int(&outcome["relations_duplicate"], "not reported".into()),
            reading_int(&outcome["relations_refused"], "not reported".into()),
        ),
        format!(
            "  extraction: source {}, windows {}, truncated {}, tokens {}, cost {}",
            outcome["source"].as_str().unwrap_or("?"),
            reading_int(&outcome["windows"], "not measured".into()),
            reading_bool(&outcome["truncated"], "not measured".into()),
            reading_int(&outcome["tokens_used"], "not reported".into()),
            reading_cost(&outcome["cost_usd"], "not reported".into()),
        ),
    ];
    // WHICH MESSAGES WERE READ (issue #48): the CLI is the only surface that can
    // show the range -- the ledger has no column for it -- and a cut transcript is
    // a fact about the extraction, so it belongs beside `truncated`.
    if !outcome["coverage"].is_null() {
        let c = &outcome["coverage"];
        out.push(format!(
            "  coverage: messages {}..{} of {}, {} B rendered at a {} B budget per window",
            c["from_message"].as_u64().unwrap_or(0),
            c["to_message"].as_u64().unwrap_or(0),
            c["total_messages"].as_u64().unwrap_or(0),
            c["bytes"].as_u64().unwrap_or(0),
            c["budget_bytes"].as_u64().unwrap_or(0),
        ));
    }
    out
}

/// What a pass PLANNED to cost, with the basis it rests on.
///
/// `null` IS THE ANSWER when no successful attempt recorded usage (the 0028 rule):
/// printing 0 would make a backlog look free. The basis line prints how many
/// attempts recorded each number, so a mean over one attempt cannot pose as a mean
/// over forty.
fn estimate_lines(est: &serde_json::Value) -> Vec<String> {
    if est.is_null() {
        return vec!["  estimate: not available (the daemon returned none)".to_string()];
    }
    let sessions = est["sessions"].as_u64().unwrap_or(0);
    let windows = est["windows"].as_u64().unwrap_or(0);
    let bytes = est["bytes"].as_u64().unwrap_or(0);
    let budget = est["budget_bytes"].as_u64().unwrap_or(0);
    let basis = &est["basis"];
    let ok_attempts = basis["ok_attempts"].as_u64().unwrap_or(0);
    let absent = format!("not recorded by any of {ok_attempts} successful attempt(s)");
    vec![
        format!(
            "  estimate: {windows} model call(s) for {sessions} session(s) — {bytes} B at a \
             {budget} B budget per call; tokens {}, cost {}",
            reading_number(&est["tokens_used"], absent.clone()),
            reading_cost(&est["cost_usd"], absent),
        ),
        format!(
            "  estimate basis: {ok_attempts} successful attempt(s); windows recorded on {}, \
             tokens on {}, cost on {}",
            basis["windows_recorded"].as_u64().unwrap_or(0),
            basis["tokens_recorded"].as_u64().unwrap_or(0),
            basis["cost_recorded"].as_u64().unwrap_or(0),
        ),
    ]
}

/// What a pass REALLY recorded, in the same units as the estimate (the ledger rows
/// it wrote), with the other half of the 0028 rule beside it.
fn actual_line(actual: &serde_json::Value) -> String {
    if actual.is_null() {
        return "nothing recorded (this pass wrote no ledger rows)".to_string();
    }
    let rows = actual["rows"].as_u64().unwrap_or(0);
    if rows == 0 {
        return "no ledger rows written by this pass".to_string();
    }
    let miss = |key: &str| actual[key].as_u64().unwrap_or(0);
    format!(
        "{rows} ledger row(s); windows {}, tokens {}, cost {}",
        reading_int(
            &actual["windows"],
            format!("not recorded on {} row(s)", miss("windows_not_recorded")),
        ),
        reading_int(
            &actual["tokens_used"],
            format!(
                "not recorded on {} row(s)",
                miss("tokens_used_not_recorded")
            ),
        ),
        reading_cost(
            &actual["cost_usd"],
            format!("not recorded on {} row(s)", miss("cost_usd_not_recorded")),
        ),
    )
}

/// The human report of a backlog response, line by line.
///
/// PURE, and it re-states nothing: the counts it prints are the ones the daemon
/// sent (its route classified each attempt with the ledger in hand), so the table
/// and the exit code cannot disagree with the response.
fn batch_report_lines(resp: &serde_json::Value) -> Vec<String> {
    let dry_run = resp["dry_run"].as_bool().unwrap_or(false);
    let planned = resp["planned"].as_u64().unwrap_or(0);
    let total = resp["undistilled_total"].as_u64().unwrap_or(0);
    let mut out = vec![format!(
        "distillation backlog: {planned} of {total} undistilled session(s) planned, extractor {}{}",
        resp["extractor"].as_str().unwrap_or("?"),
        if dry_run {
            " — DRY RUN: no model call, nothing written"
        } else {
            ""
        },
    )];
    out.extend(estimate_lines(&resp["estimated"]));
    let results = resp["results"].as_array().cloned().unwrap_or_default();
    for r in &results {
        out.push(format!(
            "  [{:<12}] {}{}",
            r["status"].as_str().unwrap_or("?"),
            r["session_key"].as_str().unwrap_or("?"),
            r["reason"]
                .as_str()
                .map(|x| format!(" — {x}"))
                .unwrap_or_default(),
        ));
    }
    out.push(three_state_line(
        resp["attempted"].as_u64().unwrap_or(0) as u32,
        resp["ok"].as_u64().unwrap_or(0) as u32,
        resp["empty"].as_u64().unwrap_or(0) as u32,
        resp["failed"].as_u64().unwrap_or(0) as u32,
        count_status(&results, DISTILL_STATUS_NOT_RECORDED),
    ));
    if !dry_run {
        out.push(format!("  actual: {}", actual_line(&resp["actual"])));
    }
    if total > planned {
        out.push(format!(
            "  {} more undistilled session(s) are beyond this pass's limit",
            total - planned
        ));
    }
    out
}

/// ONE attempt's `actual`, in the SAME units as the batch's: the quantities the
/// ledger recorded for it, plus the counts of what it did not record.
fn actual_from_row(row: Option<&serde_json::Value>) -> serde_json::Value {
    let Some(row) = row else {
        return serde_json::json!({
            "rows": 0,
            "windows": serde_json::Value::Null,
            "tokens_used": serde_json::Value::Null,
            "cost_usd": serde_json::Value::Null,
            "windows_not_recorded": 0,
            "tokens_used_not_recorded": 0,
            "cost_usd_not_recorded": 0,
        });
    };
    let missing = |key: &str| i64::from(row[key].is_null());
    serde_json::json!({
        "rows": 1,
        "windows": row["windows"],
        "tokens_used": row["tokens_used"],
        "cost_usd": row["cost_usd"],
        "windows_not_recorded": missing("windows"),
        "tokens_used_not_recorded": missing("tokens_used"),
        "cost_usd_not_recorded": missing("cost_usd"),
    })
}

/// One read of the ledger for one session, newest row first -- t6's endpoint
/// (`/api/v1/distill/log`), through the same client as everything else here.
///
/// WHY THE ERROR IS PROPAGATED INSTEAD OF BECOMING AN EMPTY LIST: a ledger that
/// cannot be READ must not look like a session with no rows. "this attempt
/// recorded nothing" (a finding about distillation) and "I could not ask" (a
/// failure of the check) are different answers, and collapsing them is how a
/// broken daemon reads as a clean ledger.
fn ledger_rows(url: &str, key: &str, limit: u32) -> Result<Vec<serde_json::Value>> {
    let path = format!(
        "/api/v1/distill/log?session={}&limit={limit}",
        urlencoding::encode(key)
    );
    let v = api_json(
        client()
            .get(format!("{url}{path}"))
            .send()
            .context("daemon unreachable (is `ruagent serve` running?)")?,
    )?;
    Ok(v["entries"].as_array().cloned().unwrap_or_default())
}

/// The newest ledger row id for one session, read BEFORE an attempt. One row is
/// written per attempt (0024), so the attempt's own row can only have a HIGHER id
/// -- which is what makes "which row did THIS attempt write?" answerable without
/// guessing from timestamps.
fn newest_ledger_id(url: &str, key: &str) -> Result<i64> {
    Ok(ledger_rows(url, key, 1)?
        .first()
        .and_then(|r| r["id"].as_i64())
        .unwrap_or(0))
}

/// The ledger row this attempt wrote, if any: the newest row for the session whose
/// id is above the pre-attempt high-water mark. `None` means the attempt recorded
/// nothing, which the caller REPORTS rather than guessing about.
fn ledger_row_since(url: &str, key: &str, before: i64) -> Result<Option<serde_json::Value>> {
    Ok(ledger_rows(url, key, 5)?
        .into_iter()
        .find(|r| r["id"].as_i64().unwrap_or(0) > before))
}

/// Print the report -- the daemon's own JSON, or the human lines -- and turn a
/// failure count into this process's exit code, in ONE place: both routes report
/// the same way and both fail the same way.
fn report_distill(resp: &serde_json::Value, json: bool, lines: Vec<String>) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(resp)?);
    } else {
        for line in lines {
            println!("{line}");
        }
    }
    let failed = resp["failed"].as_u64().unwrap_or(0);
    if failed > 0 {
        bail!(
            "{failed} of {} distillation attempt(s) failed; the reasons are in the report above",
            resp["attempted"].as_u64().unwrap_or(0)
        );
    }
    Ok(())
}

/// `ruagent distill` (t9): one session, or the whole undistilled backlog.
fn distill_cmd(
    url: &str,
    session: Option<&str>,
    unprocessed: Option<u32>,
    extractor: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    match (session, unprocessed) {
        (Some(key), _) => distill_one(url, key, extractor, dry_run, json),
        (None, Some(limit)) => distill_backlog(url, limit, extractor, dry_run, json),
        // Unreachable: clap's ArgGroup requires exactly one target (see Cmd::Distill).
        (None, None) => bail!("specify --session <key> or --unprocessed <n>"),
    }
}

/// The POST body BOTH routes take, in t4's own vocabulary (`distill_request` reads
/// it: an empty body means `ExtractPlan::explicit(Acp)` and no dry run).
///
/// `dry_run` rides the body on both routes, and the batch ALSO accepts it as a query
/// parameter and ORs the two, so a caller never has to know which one to use. The
/// extractor is passed through unvalidated on purpose: the daemon owns the
/// vocabulary and its refusal names the accepted values.
fn distill_body(extractor: Option<&str>, dry_run: bool) -> serde_json::Value {
    let mut body = serde_json::json!({ "dry_run": dry_run });
    if let Some(extractor) = extractor {
        body["extractor"] = serde_json::json!(extractor);
    }
    body
}

/// `--unprocessed N`: the BACKLOG pass (issue #47) through the batch route.
///
/// The daemon clamps N to its own per-pass ceiling and the report prints how many
/// of the undistilled total it planned, so a clamped pass is visible rather than
/// silent -- the CLI deliberately does not carry a second copy of that ceiling.
fn distill_backlog(
    url: &str,
    limit: u32,
    extractor: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let resp = api_json(
        client()
            .post(format!("{url}/api/v1/sessions/distill/batch?limit={limit}"))
            .json(&distill_body(extractor, dry_run))
            .send()
            .context("daemon unreachable (is `ruagent serve` running?)")?,
    )?;
    report_distill(&resp, json, batch_report_lines(&resp))
}

/// `--session <key>`: ONE session, through the single route (issue #46).
///
/// THE THREE STATES COME FROM THE LEDGER, NOT FROM THE RESPONSE: the route returns
/// the extraction's counts, and only `distill_log` records what became of the
/// attempt. So the CLI reads this session's high-water mark BEFORE the call and
/// looks for a row above it afterwards, then reports `not_recorded` when there is
/// none -- the same rule the batch route's own entries are built on.
fn distill_one(
    url: &str,
    key: &str,
    extractor: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<()> {
    let before = newest_ledger_id(url, key)?;
    let call = client()
        .post(format!(
            "{url}/api/v1/sessions/{}/distill",
            urlencoding::encode(key)
        ))
        .json(&distill_body(extractor, dry_run))
        .send()
        .context("daemon unreachable (is `ruagent serve` running?)")?;
    let mut outcome = serde_json::Value::Null;
    // A refusal is a 4xx whose `message` is the text t4 wrote for it (no SQL
    // vocabulary in it); it is the reason whenever the ledger recorded none.
    let error: Option<String> = match api_json(call) {
        Ok(v) => {
            outcome = v["distilled"].clone();
            None
        }
        Err(e) => Some(format!("{e:#}")),
    };
    let row = ledger_row_since(url, key, before)?;

    let (status, reason) = if dry_run && row.is_none() && error.is_none() {
        // A dry run writes NO ledger row BY DESIGN (`DistillOutcome::dry_run`), so
        // an absent row is not the `not_recorded` anomaly: nothing was recorded,
        // because nothing was meant to be written.
        (None, None)
    } else {
        let (status, reason) = classify_distill_attempt(row.as_ref(), error.as_deref());
        (Some(status), reason)
    };
    let attempted = u32::from(status.is_some());
    let ok = u32::from(status.as_deref() == Some("ok"));
    let empty = u32::from(status.as_deref() == Some("empty"));
    let failed = u32::from(status.as_deref() == Some("failed"));
    let not_recorded = u32::from(status.as_deref() == Some(DISTILL_STATUS_NOT_RECORDED));

    // THE SAME ENVELOPE the batch route returns, so `--json` has ONE shape for
    // both targets and the exit-code rule reads one field in both cases. The
    // inapplicable fields are `null` rather than absent, so a consumer can tell
    // "not estimated for a single session" from "this key does not exist".
    let envelope = serde_json::json!({
        "dry_run": dry_run,
        "session_key": key,
        "extractor": outcome["source"].as_str(),
        "planned": u32::from(!dry_run),
        "undistilled_total": serde_json::Value::Null,
        "attempted": attempted,
        "ok": ok,
        "empty": empty,
        "failed": failed,
        "status": status.clone(),
        "reason": reason.clone(),
        "estimated": serde_json::Value::Null,
        "actual": actual_from_row(row.as_ref()),
        "results": match &status {
            Some(s) => vec![serde_json::json!({
                "session_key": key,
                "status": s,
                "reason": reason.clone(),
            })],
            None => Vec::new(),
        },
        "outcome": outcome.clone(),
    });

    let lines = if dry_run {
        let mut lines = vec![format!(
            "session {key}: DRY RUN — the extraction ran; nothing was written"
        )];
        lines.extend(outcome_lines(&outcome, "would write"));
        lines.push(three_state_line(attempted, ok, empty, failed, not_recorded));
        lines
    } else {
        let word = status.as_deref().unwrap_or(DISTILL_STATUS_NOT_RECORDED);
        let mut lines = vec![format!(
            "session {key}: [{word}]{}{}",
            reason
                .as_deref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default(),
            if word == DISTILL_STATUS_NOT_RECORDED {
                " (the ledger recorded nothing for this attempt)"
            } else {
                ""
            },
        )];
        match row.as_ref() {
            // The ledger row is the receipt: print IT rather than the response's
            // echo of the same counts.
            Some(row) => lines.extend(ledger_attempt_lines(row)),
            None => lines.extend(outcome_lines(&outcome, "the extraction reported")),
        }
        lines.push(three_state_line(attempted, ok, empty, failed, not_recorded));
        lines
    };
    report_distill(&envelope, json, lines)
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
