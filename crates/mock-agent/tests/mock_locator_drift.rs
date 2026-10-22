//! THE DRIFT GUARD for the cross-package copies of the mock-agent locator (t42, audit F9).
//!
//! WHY THERE ARE COPIES AT ALL: `env!("CARGO_BIN_EXE_ruagent-mock-agent")` is the honest way
//! to find this package's binary, and it is what THIS package's own tests use -- but the macro
//! is only defined for the package that declares the binary, so `crates/daemon/tests/*` and
//! `crates/mcp/tests/*` cannot use it and carry a copy of the target-dir arithmetic.
//!
//! WHAT THIS GUARD DOES: every such copy is fenced with the two marker comments below, and this
//! test reads them out of the tree and requires every copy to be BYTE-IDENTICAL. A fix applied to
//! one copy alone therefore turns red in the package that owns the binary -- the mechanical
//! answer to "one fact, several handwritten definitions", and much cheaper than forcing the
//! copies into a new dev-only crate (see the cost/benefit note in t42's output).
//!
//! It also checks the SECOND carrier of the same message: the `#[ignore = "..."]` reason that
//! libtest prints. Rust requires an attribute value to be a literal, so the long reason cannot be
//! a reference to the const inside the block; this guard is what keeps the two spellings from
//! drifting, by requiring each of them to name the exact command that satisfies the precondition.
//!
//! THE PARSER PRINTS WHAT IT EXTRACTED (the t29 lesson: a guard that reads a second carrier is
//! blind exactly where its parser is, and that failure is SILENT). If it ever finds fewer than two
//! copies it FAILS rather than passing on an empty scan.

use std::path::{Path, PathBuf};

const BEGIN: &str = "// ---- mock-agent-locator: begin";
const END: &str = "// ---- mock-agent-locator: end ----";
/// The one command that satisfies the precondition, named by the const and by every ignore reason.
const BUILD_CMD: &str = "cargo build -p ruagent-mock-agent --bins";

fn rust_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files_under(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn block_of(text: &str) -> Option<String> {
    let start = text.find(BEGIN)?;
    let end = text.find(END)? + END.len();
    Some(text[start..end].to_string())
}

#[test]
fn the_cross_package_locator_copies_do_not_drift() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the workspace root is two levels above this package")
        .to_path_buf();

    let mut files = Vec::new();
    rust_files_under(&workspace.join("crates"), &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "the scan found no .rs files under {} -- that is a blind parser, not a clean tree",
        workspace.join("crates").display()
    );

    let mut copies: Vec<(PathBuf, String)> = Vec::new();
    let mut ignore_reasons: Vec<(PathBuf, String)> = Vec::new();
    for path in &files {
        // THIS file carries the marker TEXTS as constants, so it matches its own scan;
        // it is the guard, not a copy of the locator.
        if path.file_name().and_then(|n| n.to_str()) == Some("mock_locator_drift.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        if let Some(block) = block_of(&text) {
            let relative = path.strip_prefix(&workspace).unwrap_or(path).to_path_buf();
            println!(
                "READING t42 block {} lines={}",
                relative.display(),
                block.lines().count()
            );
            copies.push((relative, block));
        }
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("#[ignore") && line.contains("ruagent-mock-agent") {
                let relative = path.strip_prefix(&workspace).unwrap_or(path).to_path_buf();
                ignore_reasons.push((relative, line.trim().to_string()));
            }
        }
    }

    println!(
        "READING t42 blocks found={} ignore_reasons={}",
        copies.len(),
        ignore_reasons.len()
    );
    assert!(
        copies.len() >= 2,
        "found {} fenced locator copies; there must be at least two (daemon + mcp), so a smaller
         number means the markers moved and this guard would pass on an empty scan",
        copies.len()
    );
    let (first_path, first) = &copies[0];
    for (path, block) in &copies[1..] {
        assert_eq!(
            block,
            first,
            "the locator copy in {} differs from the one in {}\n\n--- {} ---\n{}\n--- {} ---\n{}",
            path.display(),
            first_path.display(),
            first_path.display(),
            first,
            path.display(),
            block
        );
    }

    for (path, reason) in &ignore_reasons {
        assert!(
            reason.contains(BUILD_CMD),
            "the ignore reason in {} does not name the command that fixes it ({BUILD_CMD}): {reason}",
            path.display()
        );
    }
    assert!(
        ignore_reasons.len() >= 9,
        "only {} mock-agent ignore reasons were parsed (8 in the daemon suite + 1 in mcp); a
         smaller number means this parser stopped seeing the carrier it is meant to guard",
        ignore_reasons.len()
    );
}
