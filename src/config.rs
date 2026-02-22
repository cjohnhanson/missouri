use std::collections::BTreeMap;

use camino::Utf8PathBuf;
use serde::Deserialize;

/// Top-level missouri.yml structure for a state.
#[derive(Debug, Deserialize)]
pub struct StateConfig {
    /// Environment variables for this state.
    #[serde(default)]
    pub env: BTreeMap<String, String>,

    /// Transitions out of this state.
    #[serde(default)]
    pub transitions: Vec<TransitionConfig>,
}

/// A transition from one state to another.
#[derive(Debug, Deserialize)]
pub struct TransitionConfig {
    /// Optional human-readable label.
    pub name: Option<String>,

    /// Command to execute.
    pub command: String,

    /// Whether to run via `sh -c` (default: true).
    #[serde(default = "default_true")]
    pub shell: bool,

    /// Relative path to the target state directory.
    pub target: Utf8PathBuf,

    /// Optional comparison overrides.
    #[serde(default)]
    pub comparators: Option<ComparatorsConfig>,
}

/// Comparison overrides for a transition.
#[derive(Debug, Deserialize)]
pub struct ComparatorsConfig {
    /// File/directory comparison overrides.
    #[serde(default)]
    pub files: Vec<FileComparatorConfig>,

    /// Environment variable comparison overrides.
    #[serde(default)]
    pub env: Vec<EnvComparatorConfig>,
}

/// Override comparison for a specific file or directory.
#[derive(Debug, Deserialize)]
pub struct FileComparatorConfig {
    /// Relative path (trailing `/` means directory subtree).
    pub path: Utf8PathBuf,

    /// Custom comparator command (receives two paths as args).
    pub command: Option<String>,

    /// If true, exclude this path from comparison entirely.
    #[serde(default)]
    pub ignore: bool,
}

/// Override comparison for a specific environment variable.
#[derive(Debug, Deserialize)]
pub struct EnvComparatorConfig {
    /// Environment variable name.
    pub name: String,

    /// Custom comparator command (receives two values as args).
    pub command: Option<String>,

    /// If true, exclude this env var from comparison entirely.
    #[serde(default)]
    pub ignore: bool,
}

fn default_true() -> bool {
    true
}

/// Parse a missouri.yml file from a string.
pub fn parse_config(content: &str) -> Result<StateConfig, serde_yml::Error> {
    serde_yml::from_str(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_config() {
        let yaml = "transitions: []";
        let config = parse_config(yaml).unwrap();
        assert!(config.env.is_empty());
        assert!(config.transitions.is_empty());
    }

    #[test]
    fn parse_full_config() {
        let yaml = r#"
env:
  APP_ENV: test
  DB_URL: "postgres://localhost/test"

transitions:
  - name: "build"
    command: "make build"
    target: "../built"
    comparators:
      files:
        - path: "dist/manifest.json"
          command: "compare-json"
        - path: "logs/"
          ignore: true
      env:
        - name: BUILD_TIMESTAMP
          ignore: true
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.env["APP_ENV"], "test");
        assert_eq!(config.transitions.len(), 1);

        let t = &config.transitions[0];
        assert_eq!(t.name.as_deref(), Some("build"));
        assert_eq!(t.command, "make build");
        assert!(t.shell);
        assert_eq!(t.target, "../built");

        let comps = t.comparators.as_ref().unwrap();
        assert_eq!(comps.files.len(), 2);
        assert_eq!(comps.files[0].command.as_deref(), Some("compare-json"));
        assert!(comps.files[1].ignore);
        assert_eq!(comps.env.len(), 1);
        assert!(comps.env[0].ignore);
    }

    #[test]
    fn parse_shell_false() {
        let yaml = r#"
transitions:
  - command: "/usr/bin/my-tool"
    shell: false
    target: "../next"
"#;
        let config = parse_config(yaml).unwrap();
        assert!(!config.transitions[0].shell);
    }

    #[test]
    fn parse_empty_is_valid() {
        // A state with no transitions and no env is valid (could be a terminal state)
        let yaml = "{}";
        let config = parse_config(yaml).unwrap();
        assert!(config.env.is_empty());
        assert!(config.transitions.is_empty());
    }
}
