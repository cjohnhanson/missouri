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

    /// Assertions to verify properties of this state.
    #[serde(default)]
    pub assertions: Vec<AssertionConfig>,
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

    /// Expected stdout (exact match) when assertions are enabled.
    pub stdout: Option<String>,

    /// Expected stderr (exact match) when assertions are enabled.
    pub stderr: Option<String>,
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

/// A side-effect-free assertion command to verify state properties.
#[derive(Debug, Deserialize)]
pub struct AssertionConfig {
    /// Optional human-readable label.
    pub name: Option<String>,

    /// Command to execute.
    pub command: String,

    /// Whether to run via `sh -c` (default: true).
    #[serde(default = "default_true")]
    pub shell: bool,

    /// Expected stdout (exact match).
    pub stdout: Option<String>,

    /// Expected stderr (exact match).
    pub stderr: Option<String>,

    /// When true, the assertion passes if the command exits non-zero.
    #[serde(default)]
    pub should_fail: bool,
}

/// Project-level missouri.yml structure (at the root config dir).
#[derive(Debug, Deserialize)]
pub struct ProjectConfig {
    /// Directory containing test states (relative to this config file).
    /// When set, state discovery starts from this directory instead of
    /// the directory containing the config.
    pub test_dir: Option<Utf8PathBuf>,

    /// Project-level environment variables (inherited by all states).
    #[serde(default)]
    pub env: BTreeMap<String, String>,

    /// Setup commands to run before any test paths.
    #[serde(default)]
    pub setup: Vec<SetupCommandConfig>,

    /// Nix packages to install via flox (simple mode).
    #[serde(default)]
    pub packages: Vec<String>,

    /// Advanced flox configuration (escape hatch to full manifest).
    #[serde(default)]
    pub flox: Option<FloxConfig>,
}

/// Advanced flox configuration pointing to an existing manifest.toml.
#[derive(Debug, Deserialize)]
pub struct FloxConfig {
    /// Path to a manifest.toml file (relative to project root).
    pub manifest: Utf8PathBuf,
}

/// A setup command that runs before test execution.
#[derive(Debug, Deserialize)]
pub struct SetupCommandConfig {
    /// Optional human-readable label.
    pub name: Option<String>,

    /// Command to execute.
    pub command: String,

    /// Whether to run via `sh -c` (default: true).
    #[serde(default = "default_true")]
    pub shell: bool,
}

fn default_true() -> bool {
    true
}

/// Parse a state-level missouri.yml file from a string.
pub fn parse_config(content: &str) -> Result<StateConfig, serde_yml::Error> {
    serde_yml::from_str(content)
}

/// Parse a project-level missouri.yml file from a string.
pub fn parse_project_config(content: &str) -> Result<ProjectConfig, serde_yml::Error> {
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
        assert!(config.assertions.is_empty());
    }

    #[test]
    fn parse_transition_output_assertions() {
        let yaml = r#"
transitions:
  - name: "echo test"
    command: "echo hello"
    target: "../next"
    stdout: "hello\n"
    stderr: ""
"#;
        let config = parse_config(yaml).unwrap();
        let t = &config.transitions[0];
        assert_eq!(t.stdout.as_deref(), Some("hello\n"));
        assert_eq!(t.stderr.as_deref(), Some(""));
    }

    #[test]
    fn parse_transition_no_output_assertions() {
        let yaml = r#"
transitions:
  - command: "do stuff"
    target: "../next"
"#;
        let config = parse_config(yaml).unwrap();
        let t = &config.transitions[0];
        assert!(t.stdout.is_none());
        assert!(t.stderr.is_none());
    }

    #[test]
    fn parse_state_assertions() {
        let yaml = r#"
assertions:
  - name: "check output"
    command: "echo hello"
    stdout: "hello\n"
  - command: "validate-data"
  - name: "check stderr"
    command: "run-check"
    stderr: "warning: none\n"
    shell: false
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.assertions.len(), 3);

        let a0 = &config.assertions[0];
        assert_eq!(a0.name.as_deref(), Some("check output"));
        assert_eq!(a0.command, "echo hello");
        assert_eq!(a0.stdout.as_deref(), Some("hello\n"));
        assert!(a0.stderr.is_none());
        assert!(a0.shell);

        let a1 = &config.assertions[1];
        assert!(a1.name.is_none());
        assert_eq!(a1.command, "validate-data");
        assert!(a1.stdout.is_none());
        assert!(a1.stderr.is_none());

        let a2 = &config.assertions[2];
        assert!(!a2.shell);
        assert_eq!(a2.stderr.as_deref(), Some("warning: none\n"));
    }

    #[test]
    fn parse_assertion_should_fail() {
        let yaml = r#"
assertions:
  - name: "expect failure"
    command: "false"
    should_fail: true
  - name: "expect success"
    command: "true"
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.assertions.len(), 2);
        assert!(config.assertions[0].should_fail);
        assert!(!config.assertions[1].should_fail);
    }

    #[test]
    fn parse_project_config_full() {
        let yaml = r#"
env:
  RUST_BACKTRACE: "1"
  APP_ENV: test

setup:
  - name: "build project"
    command: "cargo build --release"
  - command: "db-seed"
    shell: false
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(config.env.len(), 2);
        assert_eq!(config.env["RUST_BACKTRACE"], "1");
        assert_eq!(config.env["APP_ENV"], "test");

        assert_eq!(config.setup.len(), 2);
        let s0 = &config.setup[0];
        assert_eq!(s0.name.as_deref(), Some("build project"));
        assert_eq!(s0.command, "cargo build --release");
        assert!(s0.shell);

        let s1 = &config.setup[1];
        assert!(s1.name.is_none());
        assert_eq!(s1.command, "db-seed");
        assert!(!s1.shell);
    }

    #[test]
    fn parse_project_config_empty() {
        let yaml = "{}";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.env.is_empty());
        assert!(config.setup.is_empty());
    }

    #[test]
    fn parse_project_config_env_only() {
        let yaml = r#"
env:
  FOO: bar
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(config.env.len(), 1);
        assert_eq!(config.env["FOO"], "bar");
        assert!(config.setup.is_empty());
    }

    #[test]
    fn parse_project_config_setup_only() {
        let yaml = r#"
setup:
  - name: "init"
    command: "make init"
"#;
        let config = parse_project_config(yaml).unwrap();
        assert!(config.env.is_empty());
        assert_eq!(config.setup.len(), 1);
    }

    #[test]
    fn parse_project_config_packages() {
        let yaml = r#"
packages:
  - python3
  - uv
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(config.packages, vec!["python3", "uv"]);
        assert!(config.flox.is_none());
    }

    #[test]
    fn parse_project_config_flox_manifest() {
        let yaml = r#"
flox:
  manifest: "env/manifest.toml"
"#;
        let config = parse_project_config(yaml).unwrap();
        assert!(config.packages.is_empty());
        let flox = config.flox.unwrap();
        assert_eq!(flox.manifest.as_str(), "env/manifest.toml");
    }

    #[test]
    fn parse_project_config_no_sandbox() {
        let yaml = "{}";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.packages.is_empty());
        assert!(config.flox.is_none());
    }

    #[test]
    fn parse_project_config_test_dir() {
        let yaml = r#"
test_dir: tests/smoke
env:
  FOO: bar
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(config.test_dir.as_deref(), Some("tests/smoke".into()));
        assert_eq!(config.env["FOO"], "bar");
    }

    #[test]
    fn parse_project_config_no_test_dir() {
        let yaml = r#"
env:
  FOO: bar
"#;
        let config = parse_project_config(yaml).unwrap();
        assert!(config.test_dir.is_none());
    }
}
