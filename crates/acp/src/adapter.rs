//! Harness adapters: per-CLI quirks encoded in one place (design §4.2).
//!
//! M1's universal quirk is Windows program resolution: npm-installed CLIs
//! (`npx`, `claude`, `dsh`) are `.cmd` batch shims that
//! `std::process::Command` cannot execute directly — they must be wrapped
//! in `cmd /c`. Per-harness behavioral quirks (config options, resume
//! semantics) join this trait in M2.

use ruagent_core::{AgentCard, HarnessKind};

use crate::AcpError;

/// A resolved spawn command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnSpec {
    pub program: String,
    pub args: Vec<String>,
}

/// Resolve `program` for spawning.
///
/// - Path-like programs (contain a separator) are used as-is.
/// - Otherwise PATH is searched. On Windows, `.exe` hits run directly;
///   `.cmd`/`.bat` shims are wrapped with `cmd /c` (std Command cannot
///   spawn batch files). On Unix, the first executable found wins.
pub fn resolve_program(program: &str) -> Option<SpawnSpec> {
    if program.contains('/') || program.contains('\\') {
        return Some(SpawnSpec {
            program: program.to_string(),
            args: vec![],
        });
    }
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<std::path::PathBuf> = std::env::split_paths(&path).collect();

    #[cfg(windows)]
    {
        // Prefer real executables over batch shims.
        for dir in &dirs {
            let exe = dir.join(format!("{program}.exe"));
            if exe.is_file() {
                return Some(SpawnSpec {
                    program: exe.to_string_lossy().into_owned(),
                    args: vec![],
                });
            }
        }
        for dir in &dirs {
            for ext in ["cmd", "bat"] {
                let shim = dir.join(format!("{program}.{ext}"));
                if shim.is_file() {
                    return Some(SpawnSpec {
                        program: "cmd".into(),
                        args: vec!["/c".into(), shim.to_string_lossy().into_owned()],
                    });
                }
            }
        }
        None
    }

    #[cfg(not(windows))]
    {
        for dir in &dirs {
            let candidate = dir.join(program);
            if candidate.is_file() {
                return Some(SpawnSpec {
                    program: candidate.to_string_lossy().into_owned(),
                    args: vec![],
                });
            }
        }
        None
    }
}

/// The per-harness adapter trait. M1 ships one standard implementation;
/// M2 adds per-harness `set_config_option` handling, resume semantics and
/// multiplexing capabilities.
pub trait HarnessAdapter: Send + Sync {
    fn kind(&self) -> HarnessKind;

    /// Build the spawn spec for an agent card: split its command line,
    /// resolve the program (PATH + Windows quirks), append arguments.
    fn spawn_spec(&self, card: &AgentCard) -> Result<SpawnSpec, AcpError>;
}

/// The standard adapter: command-line splitting + PATH resolution.
pub struct StandardAdapter {
    kind: HarnessKind,
}

impl StandardAdapter {
    pub fn new(kind: HarnessKind) -> Self {
        Self { kind }
    }
}

impl HarnessAdapter for StandardAdapter {
    fn kind(&self) -> HarnessKind {
        self.kind
    }

    fn spawn_spec(&self, card: &AgentCard) -> Result<SpawnSpec, AcpError> {
        let (program, extra_args) = crate::split_command_line(card.spawn_command());
        let mut spec = resolve_program(&program).ok_or_else(|| {
            AcpError::Command(format!(
                "program `{program}` for agent `{}` not found on PATH",
                card.name
            ))
        })?;
        spec.args.extend(extra_args);
        Ok(spec)
    }
}

/// Adapter for a harness kind.
pub fn adapter_for(kind: HarnessKind) -> StandardAdapter {
    StandardAdapter::new(kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pathlike_passes_through() {
        let spec = resolve_program(r"C:\tools\some-agent.exe").unwrap();
        assert_eq!(spec.program, r"C:\tools\some-agent.exe");
        assert!(spec.args.is_empty());
    }

    #[test]
    fn unknown_program_is_none() {
        assert!(resolve_program("ruagent-definitely-not-real-xyz").is_none());
    }

    #[test]
    fn cargo_resolves_on_path() {
        // cargo is guaranteed present for these tests to run at all.
        let spec = resolve_program("cargo").unwrap();
        assert!(!spec.program.is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn npx_cmd_shim_wraps_with_cmd_c() {
        // npm shims exist on this machine; whichever resolution applies,
        // spawning must be possible.
        if let Some(spec) = resolve_program("npx") {
            if spec.program == "cmd" {
                assert_eq!(spec.args[0], "/c");
                assert!(spec.args[1].to_lowercase().ends_with(".cmd"));
            } else {
                assert!(spec.program.to_lowercase().ends_with(".exe"));
            }
        }
    }

    #[test]
    fn spawn_spec_appends_args() {
        // Requires the harness CLIs on PATH; skip (not fail) elsewhere —
        // CI runners do not install dsh/opencode.
        if ruagent_acp::resolve_program("dsh").is_none() {
            eprintln!("skipping: dsh not on PATH");
            return;
        }
        let mut card = ruagent_core::AgentCard {
            id: ruagent_core::AgentId::generate(),
            name: "dsh".into(),
            harness: HarnessKind::Dsh,
            command: Some("dsh --profile acp".into()),
            description: String::new(),
            model: None,
            reasoning_effort: None,
            context_window: None,
            mcp_profile: None,
            tags: vec![],
            enabled: true,
        };
        let spec = adapter_for(HarnessKind::Dsh)
            .spawn_spec(&card)
            .unwrap_or_else(|_| panic!("dsh must resolve on this machine"));
        assert!(spec.args.contains(&"--profile".to_string()));

        card.command = None; // falls back to harness default
        let spec = adapter_for(HarnessKind::Dsh).spawn_spec(&card).unwrap();
        assert!(spec.args.contains(&"acp".to_string()));
    }
}
