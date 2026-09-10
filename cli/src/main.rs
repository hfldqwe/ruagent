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
    /// Run a prompt as a one-off task and stream the result.
    Run {
        prompt: String,
        /// Agent name (default: first enabled agent).
        #[arg(long)]
        agent: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Serve { addr, root } => {
            let addr: std::net::SocketAddr = addr.parse().context("invalid listen address")?;
            let root = root.unwrap_or_else(ruagent_daemon::default_root);
            ruagent_daemon::serve(root, addr).await
        }
        Cmd::Agents => list_agents(&cli.url),
        Cmd::Status => status(&cli.url),
        Cmd::Run { prompt, agent } => run(&cli.url, &prompt, agent.as_deref()),
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(600))
        .build()
        .expect("http client builds")
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
