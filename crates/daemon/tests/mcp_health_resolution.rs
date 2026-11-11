//! t80: the MCP health check must judge THE COMMAND THE HARNESS WOULD ACTUALLY BE GIVEN.
//!
//! The defect this guards: `McpHealth::refresh` built its ping target from `entry.command` (the
//! CONFIGURED value) while injection went through `McpConfig::spawn_command`. On a machine without
//! `ruagent` on PATH the platform's own server -- configured as a bare `ruagent` -- was therefore
//! pinged as an unresolvable name, recorded `down`, and then EXCLUDED from injection by the health
//! gate: every agent lost the platform's tools, which is the 0.1.6 failure arriving through the
//! diagnostic path instead of the injection path.
//!
//! These assertions hold whether or not `ruagent` resolves in the environment running them, which
//! is what makes them safe in CI: the property is that the two paths agree, not that one particular
//! substitution happened.

use agent_client_protocol::schema::v1::McpServer;
use ruagent_daemon::config::{McpConfig, McpEntry};

fn entry(command: &str, args: &[&str]) -> McpEntry {
    McpEntry {
        command: Some(command.to_string()),
        args: args.iter().map(|s| s.to_string()).collect(),
        url: None,
        inject_for: None,
    }
}

/// The shipped shape: the platform's own server (bare name + `mcp-serve`) next to a user's own.
fn config() -> McpConfig {
    let mut cfg = McpConfig::default();
    cfg.servers
        .insert("ruagent".into(), entry("ruagent", &["mcp-serve"]));
    cfg.servers.insert(
        "mine".into(),
        entry("some-user-mcp-binary-9f3a", &["--stdio"]),
    );
    cfg.profiles
        .insert("default".into(), vec!["ruagent".into(), "mine".into()]);
    cfg
}

/// What the harness is handed: (server name, stdio program).
fn programs(servers: &[McpServer]) -> Vec<(String, String)> {
    servers
        .iter()
        .map(|s| match s {
            McpServer::Stdio(stdio) => (stdio.name.clone(), stdio.command.display().to_string()),
            other => (format!("{other:?}"), "(not stdio)".to_string()),
        })
        .collect()
}

#[test]
fn the_health_check_and_the_injection_name_the_same_program() {
    let cfg = config();
    let injected = programs(&cfg.expand_profile(Some("default"), "dsh"));
    assert_eq!(
        injected.len(),
        2,
        "neither server may be excluded: {injected:?}"
    );

    let targets = cfg.ping_targets();
    assert_eq!(targets.len(), 2, "every registered server is checked");
    for (name, target) in targets {
        let expected = injected
            .iter()
            .find(|(n, _)| n == &name)
            .map(|(_, command)| command.clone());
        assert_eq!(
            target.command, expected,
            "health and injection disagree about `{name}`: a server can then be judged \
             down while the harness would start it fine"
        );
    }
    // Asking is not a verdict: nothing is marked down by reading the targets.
    assert!(cfg.health.down_set().is_empty());
}

#[test]
fn a_users_own_server_is_never_rewritten_for_the_health_check() {
    let cfg = config();
    let target = cfg.ping_target("mine", &cfg.servers["mine"]);
    assert_eq!(
        target.command.as_deref(),
        Some("some-user-mcp-binary-9f3a"),
        "only the platform's own server is ever substituted"
    );
    assert_eq!(target.args, vec!["--stdio".to_string()]);
}
