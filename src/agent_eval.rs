//! Agent evaluation support for missouri assertions.
//!
//! An agent eval is a markdown file in the `.missouri/` config directory.
//! Missouri parses its YAML frontmatter as an [`AgentSpec`]. The markdown
//! body becomes the agent's prompt. The agent returns a verdict by calling
//! `missouri agent pass` or `missouri agent fail <details>`.
//!
//! Verdict protocol: the agent writes a sentinel file. The contents of
//! that file decide pass or fail. This needs no sidecar process and no
//! socket.

use std::path::Path;

use camino::{Utf8Path, Utf8PathBuf};
use crate::agent_cli::AgentSpec;

/// Sentinel file name written by `missouri agent pass/fail`.
pub const VERDICT_FILE: &str = ".missouri-verdict";

/// Result of an agent evaluation.
#[derive(Debug)]
pub struct EvalVerdict {
    pub passed: bool,
    pub details: Option<String>,
}

/// Load an agent eval markdown file from the config directory.
///
/// Looks for `<config_dir>/<name>.md` and parses the frontmatter as an
/// `AgentSpec`. Returns the spec and the markdown body. The body is the
/// evaluation prompt.
pub fn load_eval(
    state_dir: &Utf8Path,
    config_dir: &str,
    name: &str,
) -> Result<(AgentSpec, String), String> {
    let eval_path = state_dir.join(config_dir).join(format!("{name}.md"));
    if !eval_path.exists() {
        return Err(format!("eval file not found: {eval_path}"));
    }

    let content = std::fs::read_to_string(&eval_path)
        .map_err(|e| format!("failed to read {eval_path}: {e}"))?;

    AgentSpec::from_markdown(&content).map_err(|e| e.to_string())
        .map_err(|e| format!("failed to parse frontmatter in {eval_path}: {e}"))
}

/// Write a passing verdict sentinel.
pub fn write_pass(work_dir: &Path) -> std::io::Result<()> {
    let path = work_dir.join(VERDICT_FILE);
    std::fs::write(path, "pass\n")
}

/// Write a failing verdict sentinel with details.
pub fn write_fail(work_dir: &Path, details: &str) -> std::io::Result<()> {
    let path = work_dir.join(VERDICT_FILE);
    std::fs::write(path, format!("fail\n{details}\n"))
}

/// Read and parse a verdict sentinel file. Returns `None` when the agent
/// wrote no verdict, that is, when it called neither pass nor fail.
pub fn read_verdict(work_dir: &Path) -> Option<EvalVerdict> {
    let path = work_dir.join(VERDICT_FILE);
    let content = std::fs::read_to_string(path).ok()?;
    let mut lines = content.lines();

    match lines.next()? {
        "pass" => Some(EvalVerdict {
            passed: true,
            details: None,
        }),
        "fail" => {
            let details: String = lines.collect::<Vec<_>>().join("\n");
            Some(EvalVerdict {
                passed: false,
                details: if details.is_empty() {
                    None
                } else {
                    Some(details)
                },
            })
        }
        _ => None, // corrupt sentinel
    }
}

/// Resolve the eval markdown path for a given eval name.
pub fn eval_path(state_dir: &Utf8Path, config_dir: &str, name: &str) -> Utf8PathBuf {
    state_dir.join(config_dir).join(format!("{name}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- verdict sentinel ----

    #[test]
    fn write_and_read_pass() {
        let tmp = tempfile::tempdir().unwrap();
        write_pass(tmp.path()).unwrap();
        let verdict = read_verdict(tmp.path()).unwrap();
        assert!(verdict.passed);
        assert!(verdict.details.is_none());
    }

    #[test]
    fn write_and_read_fail_with_details() {
        let tmp = tempfile::tempdir().unwrap();
        write_fail(tmp.path(), "command 'foo' not found on PATH").unwrap();
        let verdict = read_verdict(tmp.path()).unwrap();
        assert!(!verdict.passed);
        assert_eq!(
            verdict.details.as_deref(),
            Some("command 'foo' not found on PATH")
        );
    }

    #[test]
    fn write_and_read_fail_no_details() {
        let tmp = tempfile::tempdir().unwrap();
        write_fail(tmp.path(), "").unwrap();
        let verdict = read_verdict(tmp.path()).unwrap();
        assert!(!verdict.passed);
        assert!(verdict.details.is_none());
    }

    #[test]
    fn read_verdict_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_verdict(tmp.path()).is_none());
    }

    #[test]
    fn read_verdict_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(VERDICT_FILE), "garbage\n").unwrap();
        assert!(read_verdict(tmp.path()).is_none());
    }

    // ---- load_eval ----

    #[test]
    fn load_eval_with_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let config = root.join(".missouri");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("eval-commands.md"),
            "---\nmodel: haiku\nmax_turns: 5\n---\n\nCheck all commands exist.\n",
        )
        .unwrap();

        let (spec, body) = load_eval(root, ".missouri", "eval-commands").unwrap();
        assert_eq!(spec.model.as_deref(), Some("haiku"));
        assert_eq!(spec.max_turns, Some(5));
        assert!(body.contains("Check all commands exist."));
    }

    #[test]
    fn load_eval_no_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let config = root.join(".missouri");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("eval-simple.md"),
            "Just verify the file exists.\n",
        )
        .unwrap();

        let (spec, body) = load_eval(root, ".missouri", "eval-simple").unwrap();
        assert!(spec.model.is_none());
        assert!(body.contains("verify the file exists"));
    }

    #[test]
    fn load_eval_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let result = load_eval(root, ".missouri", "nonexistent");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    // ---- eval_path ----

    #[test]
    fn eval_path_construction() {
        let path = eval_path(
            Utf8Path::new("/project/tests/initialized"),
            ".missouri",
            "eval-commands",
        );
        assert_eq!(
            path,
            Utf8PathBuf::from("/project/tests/initialized/.missouri/eval-commands.md")
        );
    }
}
