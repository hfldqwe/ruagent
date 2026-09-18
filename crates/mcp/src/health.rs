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
