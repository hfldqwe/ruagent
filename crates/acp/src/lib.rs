//! ruagent-acp: ACP client (JSON-RPC over stdio) and harness adapters.
//!
//! The M1 execution model is connection-per-run: spawn a fresh agent
//! subprocess per run, initialize, create a session (with MCP injection),
//! stream one prompt to completion. Session resume and multiplexed
//! connections arrive in M2 (design §5, §4.2).

pub mod adapter;
pub mod chat;
pub mod fs_tools;
pub mod map;
pub mod permission;
pub mod run;

pub use adapter::{SpawnSpec, StandardAdapter, adapter_for, resolve_program};
pub use chat::{ChatCommand, ChatOptions, ChatSession, start_chat};
pub use run::{RunOptions, RunOutcome, run_once, split_command_line};

/// Errors from the ACP layer.
#[derive(Debug, thiserror::Error)]
pub enum AcpError {
    #[error("ACP protocol error: {0}")]
    Protocol(#[from] agent_client_protocol::Error),
    #[error("invalid agent command `{0}`")]
    Command(String),
    /// `session/new` was refused while MCP servers were injected.
    ///
    /// The harness's own message for this is an opaque
    /// `Internal error: { "details": "mcp-client(mcp): initial connection or
    /// tool synchronization failed" }` which names neither the server nor the
    /// command, so the user cannot act on it. This variant carries the identity
    /// of everything that was injected, plus the harness's raw error.
    #[error("{}", mcp_injection_message(servers, raw))]
    // NB: the field is deliberately NOT named "source" — thiserror treats that
    // as the error's source and demands std::error::Error, which the harness's
    // stringified message is not.
    McpInjection { servers: String, raw: String },
}

/// One line per injected MCP server: `name (stdio: cmd args)` / `name (http: url)`.
///
/// Pure and dependency-free so both the run and chat drivers can use it, and so
/// the wording is testable without a harness.
pub fn describe_mcp_servers(servers: &[agent_client_protocol::schema::v1::McpServer]) -> String {
    use agent_client_protocol::schema::v1::McpServer;
    if servers.is_empty() {
        return "（未注入）".to_string();
    }
    servers
        .iter()
        .map(|s| match s {
            McpServer::Stdio(x) => {
                let args = x.args.join(" ");
                if args.is_empty() {
                    format!("{}（stdio: {}）", x.name, x.command.display())
                } else {
                    format!("{}（stdio: {} {}）", x.name, x.command.display(), args)
                }
            }
            McpServer::Http(x) => format!("{}（http: {}）", x.name, x.url),
            McpServer::Sse(x) => format!("{}（sse: {}）", x.name, x.url),
            // ^ url is a String on both; command on Stdio is a PathBuf.
            other => format!("{other:?}"),
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// The user-facing wording for a refused session: which server, which command,
/// the raw harness error, and what the symptom usually means.
pub fn mcp_injection_message(servers: &str, raw: &str) -> String {
    format!(
        "会话建立失败（本会话注入了 MCP 服务器）：注入的服务器 = {servers}；harness 原始错误 = {raw}。\
若原始错误提到 mcp-client / initial connection / tool synchronization，通常是上面某条 command \
没能启动或没能完成握手 —— 检查该路径是否存在、是否可执行、是否正在被 cargo 重建\
（CARGO_TARGET_DIR 指向的 exe 会被每次构建覆盖；daemon 在跑时它还被锁住，链接会失败）。"
    )
}

/// The error raised when `session/new` fails with MCP servers injected.
pub fn mcp_injection_error(
    servers: &[agent_client_protocol::schema::v1::McpServer],
    source: impl std::fmt::Display,
) -> AcpError {
    mcp_injection_error_from(describe_mcp_servers(servers), source)
}

/// Same, with the server list already rendered — the connect closures carry it
/// out through a slot because the ACP library fixes their error type.
pub fn mcp_injection_error_from(servers: String, source: impl std::fmt::Display) -> AcpError {
    AcpError::McpInjection {
        servers,
        raw: source.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{McpServer, McpServerHttp, McpServerStdio};

    fn stdio(name: &str, command: &str, args: &[&str]) -> McpServer {
        let mut s = McpServerStdio::new(name, command);
        s = s.args(args.iter().map(|a| a.to_string()).collect::<Vec<_>>());
        McpServer::Stdio(s)
    }

    #[test]
    fn describe_names_every_injected_server_and_its_command() {
        let servers = vec![
            stdio("ruagent", "D:/rust_cache/debug/ruagent.exe", &["mcp-serve"]),
            McpServer::Http(McpServerHttp::new("fetch", "https://mcp.example.com/sse")),
        ];
        let text = describe_mcp_servers(&servers);
        assert!(text.contains("ruagent"), "{text}");
        assert!(text.contains("ruagent.exe mcp-serve"), "{text}");
        assert!(text.contains("fetch"), "{text}");
        assert!(text.contains("https://mcp.example.com/sse"), "{text}");
    }

    #[test]
    fn describe_says_so_when_nothing_was_injected() {
        assert_eq!(describe_mcp_servers(&[]), "（未注入）");
    }

    /// Regression (t63): the user's failed run reported only
    /// "ACP protocol error: Internal error: { \"details\": \"mcp-client(mcp):
    /// initial connection or tool synchronization failed\" }" — no server, no
    /// command, nothing to act on. The error ruagent produces must name them.
    #[test]
    fn a_refused_session_names_the_server_the_command_and_the_raw_error() {
        let servers = vec![stdio(
            "ruagent",
            "D:/rust_cache/debug/ruagent.exe",
            &["mcp-serve"],
        )];
        let err = mcp_injection_error(
            &servers,
            r#"Internal error: { "details": "mcp-client(mcp): initial connection or tool synchronization failed" }"#,
        );
        let text = err.to_string();
        // Which server, which command.
        assert!(text.contains("ruagent"), "{text}");
        assert!(text.contains("ruagent.exe mcp-serve"), "{text}");
        // The raw harness error is preserved, not swallowed.
        assert!(text.contains("mcp-client(mcp)"), "{text}");
        // And the user is told what the symptom usually means.
        assert!(text.contains("CARGO_TARGET_DIR"), "{text}");
        // It must not be the bare opaque string.
        assert_ne!(
            text,
            r#"ACP protocol error: Internal error: { "details": "mcp-client(mcp): initial connection or tool synchronization failed" }"#
        );
    }

    #[test]
    fn a_refused_session_without_injection_still_reads_cleanly() {
        let err = mcp_injection_error_from("（未注入）".to_string(), "boom");
        let text = err.to_string();
        assert!(text.contains("（未注入）"), "{text}");
        assert!(text.contains("boom"), "{text}");
    }
}
