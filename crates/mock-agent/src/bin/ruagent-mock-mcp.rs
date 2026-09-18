//! ruagent-mock-mcp: minimal MCP stdio server for tests.
//!
//! Hand-rolled newline-delimited JSON-RPC (no rmcp): answers
//! `initialize` (echoing the requested protocol version), acknowledges
//! `notifications/initialized`, and serves `tools/list` with two
//! tools. The daemon's health check (issue #24) proves itself against
//! it — a real rmcp client against a real (if tiny) wire server.

use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        let Some(id) = v.get("id").cloned() else {
            continue; // notification — nothing to answer
        };
        let reply = match v.get("method").and_then(|m| m.as_str()) {
            Some("initialize") => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": v.get("params").and_then(|p| p.get("protocolVersion"))
                        .and_then(|s| s.as_str()).unwrap_or("2025-06-18"),
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "mock-mcp", "version": "0.1.0" }
                }
            }),
            Some("tools/list") => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "tools": [
                        { "name": "mock_probe", "description": "mock tool",
                          "inputSchema": { "type": "object", "properties": {} } },
                        { "name": "mock_second", "description": "another mock tool",
                          "inputSchema": { "type": "object", "properties": {} } }
                    ]
                }
            }),
            _ => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": "method not found" }
            }),
        };
        let _ = writeln!(out, "{reply}");
        let _ = out.flush();
    }
}
