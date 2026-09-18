//! Live registry editing: create/update/delete runtimes and roles by
//! round-tripping `agents.toml` with `toml_edit` (comments, ordering and
//! unrelated entries survive every edit). The file stays the source of
//! truth — the panel edits it through the daemon, humans can still edit
//! it by hand between (and after) any API write.
//!
//! The [`Editor`] is intentionally dumb about runtime state: it only
//! reads and writes the file. The API layer decides what a valid edit
//! is (referenced runtimes must exist, etc.), re-parses the result, and
//! hot-reloads the [`crate::runs::RunManager`] registry.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use toml_edit::{DocumentMut, Item, Table, TableLike, value};

/// Field set of a runtime entry (`[runtime.X]`) the API can write.
/// `None` fields are left untouched on update.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuntimePatch {
    pub harness: Option<String>,
    pub command: Option<String>,
    pub description: Option<String>,
    pub mcp_profile: Option<String>,
    pub models: Option<Vec<String>>,
    pub enabled: Option<bool>,
}

/// Field set of an agent/role entry (`[agent.X]`).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AgentPatch {
    pub prompt: Option<String>,
    pub description: Option<String>,
    pub model: Option<String>,
    pub mcp_profile: Option<String>,
    pub runtimes: Option<Vec<String>>,
    pub runtime: Option<String>,
    /// Canonical session-option defaults (`options = { mode = "plan" }`).
    /// `Some(empty)` removes the key; `None` leaves it alone.
    pub options: Option<std::collections::BTreeMap<String, String>>,
    pub enabled: Option<bool>,
}

/// Round-trip editor over one `agents.toml`.
pub struct Editor {
    path: PathBuf,
    /// Serializes edits (two concurrent POSTs must not interleave
    /// read-modify-write cycles).
    lock: std::sync::Mutex<()>,
}

impl Editor {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            lock: std::sync::Mutex::new(()),
        }
    }

    /// Path of the edited file (tests assert on it).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Names of every `[runtime.*]` entry (validation + pickers).
    pub fn runtime_names(&self) -> Result<Vec<String>> {
        let doc = self.load()?;
        let mut names = Vec::new();
        if let Some(t) = doc
            .as_table()
            .get("runtime")
            .and_then(|r| r.as_table_like())
        {
            for (name, item) in t.iter() {
                if item.as_table_like().is_some() {
                    names.push(name.to_owned());
                }
            }
        }
        Ok(names)
    }

    /// Names of every `[agent.*]` entry.
    pub fn agent_names(&self) -> Result<Vec<String>> {
        let doc = self.load()?;
        let mut names = Vec::new();
        if let Some(t) = doc.as_table().get("agent").and_then(|a| a.as_table_like()) {
            for (name, item) in t.iter() {
                if item.as_table_like().is_some() {
                    names.push(name.to_owned());
                }
            }
        }
        Ok(names)
    }

    fn load(&self) -> Result<DocumentMut> {
        let text = std::fs::read_to_string(&self.path)
            .with_context(|| format!("reading {}", self.path.display()))?;
        text.parse::<DocumentMut>()
            .with_context(|| format!("parsing {}", self.path.display()))
    }

    fn save(&self, doc: &DocumentMut) -> Result<()> {
        // temp + rename: a crash mid-write never truncates the file.
        let tmp = self.path.with_extension("toml.tmp");
        std::fs::write(&tmp, doc.to_string())
            .with_context(|| format!("writing {}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("replacing {}", self.path.display()))?;
        Ok(())
    }

    /// A named sub-table under `runtime` or `agent`, creating the parent
    /// (and the child) as needed.
    fn entry_table<'a>(doc: &'a mut DocumentMut, layer: &str, name: &str) -> &'a mut dyn TableLike {
        let parent = doc
            .as_table_mut()
            .entry(layer)
            .or_insert(Item::Table({
                let mut t = Table::new();
                t.set_implicit(true);
                t
            }))
            .as_table_mut()
            .expect("layer is a table");
        parent
            .entry(name)
            .or_insert(Item::Table(Table::new()))
            .as_table_like_mut()
            .expect("entry is a table")
    }

    /// Set every Some field; drop the entry's stale key when a field is
    /// explicitly cleared (None means "leave alone" everywhere).
    fn set_str(t: &mut dyn TableLike, key: &str, v: &Option<String>) {
        if let Some(v) = v {
            t.insert(key, value(v.clone()));
        }
    }

    fn set_string_list(t: &mut dyn TableLike, key: &str, v: &Option<Vec<String>>) {
        if let Some(v) = v {
            let arr: toml_edit::Array = v.iter().cloned().collect();
            t.insert(key, value(arr));
        }
    }

    // -- runtimes --------------------------------------------------------

    /// Create `[runtime.<name>]`. Fails if it already exists.
    pub fn create_runtime(&self, name: &str, patch: &RuntimePatch) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        check_name(name)?;
        let mut doc = self.load()?;
        if find_table(doc.as_table_mut(), "runtime", name).is_some() {
            bail!("runtime `{name}` already exists");
        }
        let t = Self::entry_table(&mut doc, "runtime", name);
        Self::set_str(t, "harness", &patch.harness);
        Self::set_str(t, "command", &patch.command);
        Self::set_str(t, "description", &patch.description);
        Self::set_str(t, "mcp_profile", &patch.mcp_profile);
        Self::set_string_list(t, "models", &patch.models);
        if let Some(e) = patch.enabled {
            t.insert("enabled", value(e));
        }
        self.save(&doc)
    }

    /// Update `[runtime.<name>]` (only the provided fields). Fails if it
    /// does not exist.
    pub fn update_runtime(&self, name: &str, patch: &RuntimePatch) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        let mut doc = self.load()?;
        let Some(t) = find_table(doc.as_table_mut(), "runtime", name) else {
            bail!("runtime `{name}` not found");
        };
        Self::set_str(t, "harness", &patch.harness);
        Self::set_str(t, "command", &patch.command);
        Self::set_str(t, "description", &patch.description);
        Self::set_str(t, "mcp_profile", &patch.mcp_profile);
        Self::set_string_list(t, "models", &patch.models);
        if let Some(e) = patch.enabled {
            t.insert("enabled", value(e));
        }
        self.save(&doc)
    }

    /// Delete `[runtime.<name>]`. Fails if any agent references it.
    pub fn delete_runtime(&self, name: &str) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        let mut doc = self.load()?;
        if find_table(doc.as_table_mut(), "runtime", name).is_none() {
            bail!("runtime `{name}` not found");
        }
        if let Some(users) = runtime_users(doc.as_table(), name) {
            bail!(
                "runtime `{name}` is used by agent(s) {} — remove them first or edit their runtimes",
                users.join(", ")
            );
        }
        doc.as_table_mut()
            .get_mut("runtime")
            .and_then(|r| r.as_table_mut())
            .expect("runtime layer")
            .remove(name);
        self.save(&doc)
    }

    // -- agents (roles) --------------------------------------------------

    /// Create `[agent.<name>]`. Fails if it already exists.
    pub fn create_agent(&self, name: &str, patch: &AgentPatch) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        check_name(name)?;
        let mut doc = self.load()?;
        if find_table(doc.as_table_mut(), "agent", name).is_some() {
            bail!("agent `{name}` already exists");
        }
        let t = Self::entry_table(&mut doc, "agent", name);
        Self::apply_agent(t, patch);
        // Reference check INSIDE the lock, on the merged entry: a
        // concurrent delete_runtime cannot slip a dangling reference
        // between check and write (issue #41).
        Self::check_agent_refs(&doc, name)?;
        self.save(&doc)
    }

    /// Update `[agent.<name>]` (only the provided fields). When
    /// `runtimes`/`runtime` are set on a legacy self-contained entry
    /// (one carrying `harness`/`command`), those stale keys are removed —
    /// the entry becomes a proper two-layer role.
    pub fn update_agent(&self, name: &str, patch: &AgentPatch) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        let mut doc = self.load()?;
        let Some(t) = find_table(doc.as_table_mut(), "agent", name) else {
            bail!("agent `{name}` not found");
        };
        Self::apply_agent(t, patch);
        // Same in-lock reference check as create (issue #41) — on the
        // merged entry, so a partial patch is validated as it lands.
        Self::check_agent_refs(&doc, name)?;
        self.save(&doc)
    }

    /// Every runtime the (merged) `[agent.<name>]` entry references must
    /// exist in the same document — checked under the edit lock so the
    /// validation sees exactly what is about to be written (issue #41).
    fn check_agent_refs(doc: &DocumentMut, name: &str) -> Result<()> {
        let table = doc.as_table();
        let Some(t) = table
            .get("agent")
            .and_then(|a| a.as_table_like())
            .and_then(|a| a.get(name))
            .and_then(|i| i.as_table_like())
        else {
            return Ok(());
        };
        let mut refs: Vec<String> = Vec::new();
        if let Some(list) = t.get("runtimes").and_then(|v| v.as_array()) {
            refs.extend(list.iter().filter_map(|v| v.as_str().map(String::from)));
        }
        if let Some(s) = t.get("runtime").and_then(|v| v.as_str()) {
            refs.push(s.to_string());
        }
        let runtimes = table.get("runtime").and_then(|r| r.as_table_like());
        for r in &refs {
            if !runtimes.is_some_and(|rt| rt.contains_key(r)) {
                bail!("runtime `{r}` is not defined — create it first");
            }
        }
        Ok(())
    }

    fn apply_agent(t: &mut dyn TableLike, patch: &AgentPatch) {
        Self::set_str(t, "prompt", &patch.prompt);
        Self::set_str(t, "description", &patch.description);
        Self::set_str(t, "model", &patch.model);
        Self::set_str(t, "mcp_profile", &patch.mcp_profile);
        Self::set_string_list(t, "runtimes", &patch.runtimes);
        Self::set_str(t, "runtime", &patch.runtime);
        if let Some(opts) = &patch.options {
            if opts.is_empty() {
                t.remove("options");
            } else {
                let mut tbl = toml_edit::InlineTable::new();
                for (k, v) in opts {
                    tbl.insert(k.as_str(), v.clone().into());
                }
                t.insert("options", value(tbl));
            }
        }
        if let Some(e) = patch.enabled {
            t.insert("enabled", value(e));
        }
        if patch.runtimes.is_some() || patch.runtime.is_some() {
            // Two-layer form: harness/command belong to the runtime, not
            // the role. Stale copies from a legacy entry would mislead a
            // human reading the file (the parser ignores them anyway).
            t.remove("harness");
            t.remove("command");
        }
    }

    /// Delete `[agent.<name>]`.
    pub fn delete_agent(&self, name: &str) -> Result<()> {
        let _g = self.lock.lock().expect("registry edit lock");
        let mut doc = self.load()?;
        if find_table(doc.as_table_mut(), "agent", name).is_none() {
            bail!("agent `{name}` not found");
        }
        doc.as_table_mut()
            .get_mut("agent")
            .and_then(|a| a.as_table_mut())
            .expect("agent layer")
            .remove(name);
        self.save(&doc)
    }
}

/// Find `[<layer>.<name>]` as a table-like, if present.
fn find_table<'a>(
    root: &'a mut dyn TableLike,
    layer: &str,
    name: &str,
) -> Option<&'a mut dyn TableLike> {
    root.get_mut(layer)?
        .as_table_like_mut()?
        .get_mut(name)?
        .as_table_like_mut()
}

/// Names that are safe as TOML keys and URL path segments.
fn check_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("name must not be empty");
    }
    if name
        .chars()
        .any(|c| "[]=.\"'#\r\n/\\ ".contains(c) || c.is_control())
    {
        bail!("name `{name}` contains characters not allowed in agent/runtime names");
    }
    Ok(())
}

/// Agents whose `runtime`/`runtimes` reference the given runtime.
fn runtime_users(root: &dyn TableLike, runtime: &str) -> Option<Vec<String>> {
    let agents = root.get("agent")?.as_table_like()?;
    let mut users = Vec::new();
    for (name, item) in agents.iter() {
        let Some(t) = item.as_table_like() else {
            continue;
        };
        let default = t.get("runtime").and_then(|v| v.as_str());
        let list = t
            .get("runtimes")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if default == Some(runtime) || list.iter().any(|r| r == &runtime.to_string()) {
            users.push(name.to_owned());
        }
    }
    (!users.is_empty()).then_some(users)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn editor() -> (Editor, TempDir) {
        let dir = std::env::temp_dir().join(format!(
            "ruagent-reg-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("agents.toml"),
            "# my config — comments must survive

[agent.architect]
prompt = \"review\"
runtime = \"dsh\"
runtimes = [\"dsh\"]
",
        )
        .unwrap();
        (Editor::new(dir.join("agents.toml")), TempDir(dir))
    }

    fn parse(path: &Path) -> Vec<ruagent_core::AgentCard> {
        let text = std::fs::read_to_string(path).unwrap();
        // through the real parser: an edit must always yield a file the
        // boot path can read back
        crate::config::parse_agents(&text).unwrap()
    }

    #[test]
    fn create_update_delete_runtime_preserves_comments() {
        let (ed, _g) = editor();
        ed.create_runtime(
            "codex",
            &RuntimePatch {
                harness: Some("claude-code".into()),
                command: Some("codex --acp".into()),
                description: Some("Codex".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(text.starts_with("# my config"), "header comment lost");
        assert!(text.contains("[runtime.codex]"));

        ed.update_runtime(
            "codex",
            &RuntimePatch {
                command: Some("codex --acp --new".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(text.contains("codex --acp --new"));
        assert!(text.contains("# my config"));

        ed.delete_runtime("codex").unwrap();
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(!text.contains("[runtime.codex]"));
    }

    #[test]
    fn runtime_in_use_cannot_be_deleted() {
        let (ed, _g) = editor();
        ed.create_runtime(
            "dsh",
            &RuntimePatch {
                command: Some("dsh --profile acp".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let err = ed.delete_runtime("dsh").unwrap_err();
        assert!(err.to_string().contains("architect"));
    }

    #[test]
    fn agent_crud_and_legacy_promotion() {
        let (ed, _g) = editor();
        ed.create_runtime(
            "dsh",
            &RuntimePatch {
                command: Some("dsh --profile acp".into()),
                ..Default::default()
            },
        )
        .unwrap();
        ed.create_agent(
            "writer",
            &AgentPatch {
                prompt: Some("write things".into()),
                description: Some("写作".into()),
                runtimes: Some(vec!["dsh".into()]),
                runtime: Some("dsh".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let cards = parse(ed.path());
        assert!(
            cards
                .iter()
                .any(|c| c.name == "writer" && c.prompt.as_deref() == Some("write things"))
        );

        // update: change prompt only
        ed.update_agent(
            "writer",
            &AgentPatch {
                prompt: Some("write better things".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let cards = parse(ed.path());
        assert!(
            cards
                .iter()
                .any(|c| c.name == "writer" && c.prompt.as_deref() == Some("write better things"))
        );

        // session-option defaults round-trip as an inline table
        let mut opts = std::collections::BTreeMap::new();
        opts.insert("mode".to_string(), "plan".to_string());
        opts.insert("effort".to_string(), "high".to_string());
        ed.update_agent(
            "writer",
            &AgentPatch {
                options: Some(opts),
                ..Default::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(
            text.contains("options = {"),
            "options inline table missing: {text}"
        );
        let cards = parse(ed.path());
        let writer = cards.iter().find(|c| c.name == "writer").unwrap();
        assert_eq!(writer.options.get("mode").map(String::as_str), Some("plan"));
        assert_eq!(
            writer.options.get("effort").map(String::as_str),
            Some("high")
        );

        // Some(empty) clears them
        ed.update_agent(
            "writer",
            &AgentPatch {
                options: Some(Default::default()),
                ..Default::default()
            },
        )
        .unwrap();
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(!text.contains("options"));
        let cards = parse(ed.path());
        assert!(
            cards
                .iter()
                .find(|c| c.name == "writer")
                .unwrap()
                .options
                .is_empty()
        );

        // delete
        ed.delete_agent("writer").unwrap();
        let cards = parse(ed.path());
        assert!(!cards.iter().any(|c| c.name == "writer"));
    }

    #[test]
    fn bad_names_rejected() {
        let (ed, _g) = editor();
        assert!(ed.create_runtime("a.b", &RuntimePatch::default()).is_err());
        assert!(ed.create_runtime("", &RuntimePatch::default()).is_err());
        assert!(ed.create_runtime("a b", &RuntimePatch::default()).is_err());
        assert!(ed.create_agent("x/y", &AgentPatch::default()).is_err());
    }

    #[test]
    fn duplicate_creation_rejected() {
        let (ed, _g) = editor();
        ed.create_runtime(
            "dsh",
            &RuntimePatch {
                command: Some("dsh".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            ed.create_runtime(
                "dsh",
                &RuntimePatch {
                    command: Some("dsh2".into()),
                    ..Default::default()
                },
            )
            .is_err()
        );
    }

    #[test]
    fn agent_refs_missing_runtime_rejected_in_lock() {
        // Issue #41: reference validation runs inside the edit lock, on
        // the entry about to be written — so create and update can never
        // write a dangling runtime reference (a delete that interleaves
        // between an outside check and the write is no longer possible).
        let (ed, _g) = editor();
        ed.create_runtime(
            "dsh",
            &RuntimePatch {
                command: Some("dsh --profile acp".into()),
                ..Default::default()
            },
        )
        .unwrap();

        // Create referencing an unknown runtime: rejected, file unchanged.
        let err = ed
            .create_agent(
                "writer",
                &AgentPatch {
                    prompt: Some("write".into()),
                    runtime: Some("ghost".into()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("ghost"), "{err}");
        let text = std::fs::read_to_string(ed.path()).unwrap();
        assert!(
            !text.contains("[agent.writer]"),
            "rejected write must not land"
        );

        // Create a valid one, then update it onto an unknown runtime:
        // rejected, the entry keeps its valid reference.
        ed.create_agent(
            "writer",
            &AgentPatch {
                prompt: Some("write".into()),
                runtime: Some("dsh".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let err = ed
            .update_agent(
                "writer",
                &AgentPatch {
                    runtime: Some("ghost".into()),
                    ..Default::default()
                },
            )
            .unwrap_err();
        assert!(err.to_string().contains("ghost"), "{err}");
        let cards = parse(ed.path());
        let writer = cards.iter().find(|c| c.name == "writer").unwrap();
        assert_eq!(
            writer.runtime.as_deref(),
            Some("dsh"),
            "update must not land"
        );

        // A partial patch that touches no runtimes leaves the (valid)
        // references untouched — the merged entry still checks clean.
        ed.update_agent(
            "writer",
            &AgentPatch {
                prompt: Some("write more".into()),
                ..Default::default()
            },
        )
        .unwrap();
    }
}
