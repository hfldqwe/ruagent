//! MCP server health checks (issue #24): spawn each stdio server (or
//! POST its http endpoint) for a live initialize + `tools/list`
//! roundtrip. The daemon runs this on a loop and keeps the last result
//! per server; injection excludes servers whose last check failed.

use std::time::{Duration, Instant};

use rmcp::ServiceExt as _;

/// One server to ping, mapped from the registry entry.
#[derive(Debug, Clone)]
pub struct PingTarget {
    /// stdio spawn command.
    pub command: Option<String>,
    pub args: Vec<String>,
    /// streamable-http endpoint.
    pub url: Option<String>,
}

/// The result of one health check.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PingOutcome {
    pub ok: bool,
    /// Tool count from `tools/list` (ok checks only).
    pub tools: Option<u32>,
    pub latency_ms: u64,
    pub error: Option<String>,
}

/// How long one server may take to answer the full roundtrip.
const PING_TIMEOUT: Duration = Duration::from_secs(10);

/// Where a stdio command would be found, **without spawning it**.
///
/// A path-looking command must exist and be a file; a bare name is searched on
/// PATH (honouring PATHEXT on Windows), the way the OS would resolve it.
pub fn resolve_command(command: &str) -> Result<std::path::PathBuf, String> {
    let command = command.trim();
    if command.is_empty() {
        return Err("command 为空".to_string());
    }
    let looks_like_path = command.contains('/') || command.contains('\\');
    if looks_like_path {
        return match std::fs::metadata(command) {
            Ok(m) if m.is_file() => Ok(std::path::PathBuf::from(command)),
            Ok(_) => Err(format!("[{}] 不是文件", command)),
            Err(e) => Err(format!("[{}] 不存在或不可读（{e}）", command)),
        };
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    let exts: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
            .split(';')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase())
            .collect()
    } else {
        vec![String::new()]
    };
    for dir in std::env::split_paths(&path_var) {
        for ext in &exts {
            let candidate = dir.join(format!("{command}{ext}"));
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("PATH 上找不到 [{}]", command))
}

/// Cheap pre-flight: could this server even be started?
///
/// Deliberately does NOT spawn and does NOT handshake — it runs on every
/// session start, so it must cost microseconds. The deep check is [ping],
/// which the daemon's health registry runs on a loop and again on demand when
/// a session has already failed.
pub fn preflight(target: &PingTarget) -> Result<(), String> {
    match (&target.command, &target.url) {
        (Some(_), Some(_)) => Err("同时配置了 command 与 url（二选一）".to_string()),
        (Some(command), None) => resolve_command(command).map(|_| ()),
        (None, Some(url)) => {
            let url = url.trim();
            if url.starts_with("http://") || url.starts_with("https://") {
                Ok(())
            } else {
                Err(format!("url [{}] 不是 http(s) 地址", url))
            }
        }
        (None, None) => Err("既没有 command 也没有 url".to_string()),
    }
}

/// One line naming what would actually be executed — used in diagnostics so a
/// failure reads as "server ruagent (D:/.../ruagent.exe mcp-serve)" instead of
/// the harness's opaque "mcp-client(mcp): initial connection ... failed".
pub fn describe(target: &PingTarget) -> String {
    match (&target.command, &target.url) {
        (Some(c), _) if !target.args.is_empty() => format!("{c} {}", target.args.join(" ")),
        (Some(c), _) => c.clone(),
        (None, Some(u)) => u.clone(),
        (None, None) => "(未配置)".to_string(),
    }
}

/// Ping one server: initialize handshake + tools/list, bounded.
pub async fn ping(target: &PingTarget) -> PingOutcome {
    let started = Instant::now();
    let res = match (&target.command, &target.url) {
        (Some(_), Some(_)) => Err("server defines both command and url".into()),
        (Some(command), None) => ping_stdio(command, &target.args).await,
        (None, Some(url)) => ping_http(url).await,
        (None, None) => Err("server has neither command nor url".into()),
    };
    let latency_ms = started.elapsed().as_millis() as u64;
    match res {
        Ok(tools) => PingOutcome {
            ok: true,
            tools: Some(tools),
            latency_ms,
            error: None,
        },
        Err(error) => PingOutcome {
            ok: false,
            tools: None,
            latency_ms,
            error: Some(error),
        },
    }
}

async fn ping_stdio(command: &str, args: &[String]) -> Result<u32, String> {
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(args);
    let transport =
        rmcp::transport::TokioChildProcess::new(cmd).map_err(|e| format!("spawn failed: {e}"))?;
    let roundtrip = async {
        let client = ().serve(transport).await.map_err(|e| format!("initialize failed: {e}"))?;
        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| format!("tools/list failed: {e}"))?;
        let _ = client.cancel().await;
        Ok::<u32, String>(tools.len() as u32)
    };
    tokio::time::timeout(PING_TIMEOUT, roundtrip)
        .await
        .map_err(|_| "no answer within 10s".to_string())?
}

async fn ping_http(url: &str) -> Result<u32, String> {
    let transport = rmcp::transport::StreamableHttpClientTransport::from_config(
        rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig::with_uri(url),
    );
    let roundtrip = async {
        let client = ().serve(transport).await.map_err(|e| format!("initialize failed: {e}"))?;
        let tools = client
            .list_all_tools()
            .await
            .map_err(|e| format!("tools/list failed: {e}"))?;
        let _ = client.cancel().await;
        Ok::<u32, String>(tools.len() as u32)
    };
    tokio::time::timeout(PING_TIMEOUT, roundtrip)
        .await
        .map_err(|_| "no answer within 10s".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stdio(command: &str, args: &[&str]) -> PingTarget {
        PingTarget {
            command: Some(command.to_string()),
            args: args.iter().map(|s| s.to_string()).collect(),
            url: None,
        }
    }

    #[test]
    fn resolve_command_accepts_a_real_binary() {
        let exe = std::env::current_exe().expect("current exe");
        let resolved = resolve_command(&exe.display().to_string()).expect("resolves");
        assert_eq!(resolved, exe);
    }

    #[test]
    fn resolve_command_reports_a_missing_path_with_the_path_in_it() {
        let missing = if cfg!(windows) {
            r"C:\definitely-not-here-9f3a\ruagent.exe"
        } else {
            "/definitely-not-here-9f3a/ruagent"
        };
        let err = resolve_command(missing).expect_err("must fail");
        assert!(err.contains(missing), "reason must name the path: {err}");
    }

    #[test]
    fn resolve_command_reports_a_missing_bare_name() {
        let err = resolve_command("ruagent-definitely-not-on-path-9f3a").expect_err("must fail");
        assert!(err.contains("PATH"), "reason must mention PATH: {err}");
    }

    #[test]
    fn preflight_rejects_a_command_that_cannot_start() {
        let err =
            preflight(&stdio(r"C:\nope-9f3a\ruagent.exe", &["mcp-serve"])).expect_err("must fail");
        assert!(err.contains(r"C:\nope-9f3a"), "{err}");
    }

    #[test]
    fn preflight_rejects_command_and_url_together() {
        let mut t = stdio("anything", &[]);
        t.url = Some("http://127.0.0.1:1/mcp".to_string());
        assert!(preflight(&t).is_err());
    }

    #[test]
    fn preflight_rejects_a_non_http_url() {
        let t = PingTarget {
            command: None,
            args: vec![],
            url: Some("ftp://example.com".to_string()),
        };
        assert!(preflight(&t).is_err());
    }

    #[test]
    fn preflight_accepts_a_real_command_and_describe_names_it() {
        let exe = std::env::current_exe().expect("current exe");
        let t = stdio(&exe.display().to_string(), &["mcp-serve"]);
        preflight(&t).expect("a real binary passes");
        let described = describe(&t);
        assert!(described.contains("mcp-serve"), "{described}");
        assert!(described.contains("ruagent"), "{described}");
    }

    #[test]
    fn ping_reports_a_command_that_cannot_start() {
        // The deep check the daemon runs when a session has already failed: it
        // must come back with a reason, not hang or panic.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");
        let outcome = rt.block_on(ping(&stdio(r"C:\nope-9f3a\ruagent.exe", &[])));
        assert!(!outcome.ok, "{outcome:?}");
        let err = outcome.error.expect("a reason");
        // The OS message does NOT name the command ("系统找不到指定的路径。 (os error 3)"),
        // which is exactly why the caller prints describe(target) beside it.
        assert!(err.contains("spawn failed"), "{err}");
        assert!(
            describe(&stdio(r"C:\nope-9f3a\ruagent.exe", &[])).contains("nope-9f3a"),
            "the caller-side description must carry the command"
        );
    }
}
