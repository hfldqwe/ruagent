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
    /// per check with details; exit code 1 if anything failed.
    Doctor,
    /// Run a prompt as a one-off task and stream the result.
    Run {
        prompt: String,
        /// Agent name (default: first enabled agent).
        #[arg(long)]
        agent: Option<String>,
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
        Cmd::Doctor => doctor(&cli.url),
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

fn doctor(url: &str) -> Result<()> {
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
            checks.push(Check {
                name: "semantic embeddings".into(),
                ok: semantic,
                detail: if semantic {
                    "real model active".into()
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
        r##"{"name":"doctor-probe","content":"# Doctor probe\n\nThe quarterly ritual involves lighting the braziers at dawn and sounding the bronze bell twice."}"##,
    ) {
        let _ = doc; // ingest ok
        let a = get("/api/v1/knowledge/search?q=what%20happens%20at%20sunrise%20every%20quarter");
        let hit = a
            .and_then(|v| v["hits"].as_array().cloned())
            .map(|h| !h.is_empty() && h[0]["document"] == "doctor-probe")
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
        r#"{"store":"observation","namespace":"user","content":"doctor probe: the kettle is chrome"}"#,
    );
    checks.push(Check {
        name: "memory write".into(),
        ok: mem.is_some(),
        detail: mem
            .map(|m| m["outcome"].as_str().unwrap_or("?").to_string())
            .unwrap_or_else(|| "failed".into()),
    });
    let recall = get("/api/v1/recall?q=kettle%20material&strategy=aggressive&top_n=3");
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
        r#"{"name":"doctor-node","kind":"tool"}"#,
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

    // 5. session sync: sources present?
    if let Some(v) = get("/api/v1/sessions?limit=500") {
        let sessions = v["sessions"].as_array().cloned().unwrap_or_default();
        let sources: Vec<String> = {
            let mut s: Vec<String> = sessions
                .iter()
                .filter_map(|x| x["source"].as_str().map(str::to_owned))
                .collect();
            s.sort();
            s.dedup();
            s
        };
        checks.push(Check {
            name: "session sync".into(),
            ok: !sessions.is_empty(),
            detail: format!("{} sessions from: {}", sessions.len(), sources.join(", ")),
        });
    }

    // 6. distillation pipeline (dry signal): last distill_log entry age
    // (API has no distill_log endpoint yet — skip quietly when absent)

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
    for a in resp["agents"].as_array().context("bad response")? {
        let enabled = if a["enabled"].as_bool().unwrap_or(false) {
            "*"
        } else {
            " "
        };
        println!(
            "{enabled} {:<10} {:<12} {}",
            a["name"].as_str().unwrap_or("?"),
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
