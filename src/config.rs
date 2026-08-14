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

    /// When true, this state's fixture is a complete start point. A path
    /// can begin here and skip the upstream transitions.
    #[serde(default)]
    pub entrypoint: bool,

    /// Optional prose description of this state, used for doc generation.
    pub doc: Option<String>,
}

/// Network interception config for a transition.
///
/// One variant applies to each transition:
/// - `Replay { replay, hosts }` — replay the recorded responses through
///   mitmdump.
/// - `Record` — start mitmdump in record mode and save the captured flow.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum NetworkConfig {
    /// Replay a flow file that a previous run recorded.
    /// `hosts` lists the hostnames to intercept. Each hostname gets an
    /// /etc/hosts entry inside the container that points to 127.0.0.1, so
    /// the process under test can resolve it.
    Replay {
        replay: Utf8PathBuf,
        #[serde(default)]
        hosts: Vec<String>,
    },
    /// Record traffic during this transition.
    Record { record: bool },
}

/// Override comparison for a specific network request path pattern.
#[derive(Debug, Deserialize)]
pub struct NetworkComparatorConfig {
    /// URL path pattern (e.g. `"api.anthropic.com/v1/messages"` or `"*.googleapis.com/**"`).
    pub path: String,

    /// Custom comparator command.
    pub command: Option<String>,

    /// If true, exclude requests matching this path from comparison.
    #[serde(default)]
    pub ignore: bool,
}

/// A background service to run during a transition or assertion.
#[derive(Debug, Clone, Deserialize)]
pub struct ServiceConfig {
    /// Command to start the service.
    pub command: String,

    /// Whether to run via `sh -c` (default: true).
    #[serde(default = "default_true")]
    pub shell: bool,

    /// Regex pattern to extract port from stderr.
    /// Must contain one capture group for the port number.
    /// Default: `listening.*:(\d+)`
    pub port_pattern: Option<String>,

    /// Optional readiness check command.
    /// `$PORT` is available in the environment.
    /// Retried with backoff until success or timeout.
    pub ready: Option<String>,
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

    /// Optional network interception config.
    pub network: Option<NetworkConfig>,

    /// Expected stdout (exact match) when assertions are enabled.
    pub stdout: Option<String>,

    /// Expected stderr (exact match) when assertions are enabled.
    pub stderr: Option<String>,

    /// Background services to run during this transition.
    #[serde(default)]
    pub services: Vec<ServiceConfig>,

    /// Optional prose description of this transition, used for doc generation.
    pub doc: Option<String>,
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

    /// Network request comparison overrides.
    #[serde(default)]
    pub network: Vec<NetworkComparatorConfig>,
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

/// An assertion command that verifies a state property and changes
/// nothing.
///
/// Set `command` or `agent`, but not both. A command assertion runs a
/// shell command. An agent assertion starts an agent eval from a markdown
/// file in the config directory.
#[derive(Debug, Deserialize)]
pub struct AssertionConfig {
    /// Optional human-readable label.
    pub name: Option<String>,

    /// Command to execute. Required unless `agent` is set.
    #[serde(default)]
    pub command: Option<String>,

    /// Agent eval name (matches `<config_dir>/<name>.md`).
    /// Mutually exclusive with `command`.
    #[serde(default)]
    pub agent: Option<String>,

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

    /// Background services to run during this assertion.
    #[serde(default)]
    pub services: Vec<ServiceConfig>,
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

    /// Nix packages to make available via `nix shell`.
    #[serde(default)]
    pub packages: Vec<String>,

    /// Member directories for workspace mode.
    /// When set, `missouri run` visits each member and runs its tests on
    /// its own.
    #[serde(default)]
    pub members: Vec<Utf8PathBuf>,

    /// Run transitions inside Docker containers for hermetic isolation.
    #[serde(default)]
    pub docker: bool,

    /// Docker image to use for container-based execution.
    /// Default: "debian:bookworm-slim"
    pub docker_image: Option<String>,
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
        assert_eq!(a0.command.as_deref(), Some("echo hello"));
        assert_eq!(a0.stdout.as_deref(), Some("hello\n"));
        assert!(a0.stderr.is_none());
        assert!(a0.shell);

        let a1 = &config.assertions[1];
        assert!(a1.name.is_none());
        assert_eq!(a1.command.as_deref(), Some("validate-data"));
        assert!(a1.stdout.is_none());
        assert!(a1.stderr.is_none());

        let a2 = &config.assertions[2];
        assert!(!a2.shell);
        assert_eq!(a2.stderr.as_deref(), Some("warning: none\n"));
    }

    #[test]
    fn parse_assertion_agent() {
        let yaml = r#"
assertions:
  - agent: eval-skill-commands
  - agent: eval-output-quality
    name: "output quality check"
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.assertions.len(), 2);

        let a0 = &config.assertions[0];
        assert_eq!(a0.agent.as_deref(), Some("eval-skill-commands"));
        assert!(a0.command.is_none());
        assert!(a0.name.is_none());

        let a1 = &config.assertions[1];
        assert_eq!(a1.agent.as_deref(), Some("eval-output-quality"));
        assert_eq!(a1.name.as_deref(), Some("output quality check"));
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
    }

    #[test]
    fn parse_project_config_docker() {
        let yaml = "docker: true\n";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.docker);
    }

    #[test]
    fn parse_project_config_docker_default_false() {
        let yaml = "{}";
        let config = parse_project_config(yaml).unwrap();
        assert!(!config.docker);
    }

    #[test]
    fn parse_project_config_docker_with_packages() {
        let yaml = "docker: true\npackages:\n  - python3\n";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.docker);
        assert_eq!(config.packages, vec!["python3"]);
    }

    #[test]
    fn parse_project_config_no_sandbox() {
        let yaml = "{}";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.packages.is_empty());
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

    #[test]
    fn parse_project_config_members() {
        let yaml = r#"
members:
  - clc/tests/missouri
  - tisket/tests/missouri
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(
            config.members,
            vec![
                Utf8PathBuf::from("clc/tests/missouri"),
                Utf8PathBuf::from("tisket/tests/missouri"),
            ]
        );
    }

    #[test]
    fn parse_network_config_replay() {
        let yaml = r#"
transitions:
  - command: "clc dispatch test"
    target: "../next"
    network:
      replay: recordings/worker.flow
"#;
        let config = parse_config(yaml).unwrap();
        let t = &config.transitions[0];
        match t.network.as_ref().unwrap() {
            NetworkConfig::Replay { replay, .. } => {
                assert_eq!(replay.as_str(), "recordings/worker.flow");
            }
            other => panic!("expected Replay, got {other:?}"),
        }
    }

    #[test]
    fn parse_network_config_record() {
        let yaml = r#"
transitions:
  - command: "clc dispatch test"
    target: "../next"
    network:
      record: true
"#;
        let config = parse_config(yaml).unwrap();
        let t = &config.transitions[0];
        assert!(
            matches!(t.network.as_ref().unwrap(), NetworkConfig::Record { .. }),
            "expected Record variant"
        );
    }

    #[test]
    fn parse_network_config_absent() {
        let yaml = r#"
transitions:
  - command: "echo hi"
    target: "../next"
"#;
        let config = parse_config(yaml).unwrap();
        assert!(config.transitions[0].network.is_none());
    }

    #[test]
    fn parse_network_comparators() {
        let yaml = r#"
transitions:
  - command: "clc dispatch test"
    target: "../next"
    comparators:
      network:
        - path: "api.anthropic.com/v1/messages"
          command: "compare-api-calls"
        - path: "*.googleapis.com/**"
          ignore: true
"#;
        let config = parse_config(yaml).unwrap();
        let comps = config.transitions[0].comparators.as_ref().unwrap();
        assert_eq!(comps.network.len(), 2);
        assert_eq!(comps.network[0].path, "api.anthropic.com/v1/messages");
        assert_eq!(
            comps.network[0].command.as_deref(),
            Some("compare-api-calls")
        );
        assert!(!comps.network[0].ignore);
        assert_eq!(comps.network[1].path, "*.googleapis.com/**");
        assert!(comps.network[1].ignore);
        assert!(comps.network[1].command.is_none());
    }

    #[test]
    fn parse_network_comparators_absent() {
        let yaml = r#"
transitions:
  - command: "echo"
    target: "../next"
    comparators:
      files:
        - path: "out.txt"
          ignore: true
"#;
        let config = parse_config(yaml).unwrap();
        let comps = config.transitions[0].comparators.as_ref().unwrap();
        assert!(comps.network.is_empty());
    }

    #[test]
    fn parse_project_config_no_members() {
        let yaml = "{}";
        let config = parse_project_config(yaml).unwrap();
        assert!(config.members.is_empty());
    }

    #[test]
    fn parse_project_config_members_with_env() {
        let yaml = r#"
members:
  - sub/a
env:
  GLOBAL: "true"
"#;
        let config = parse_project_config(yaml).unwrap();
        assert_eq!(config.members.len(), 1);
        assert_eq!(config.env["GLOBAL"], "true");
    }

    #[test]
    fn parse_transition_with_services() {
        let yaml = r#"
transitions:
  - command: "curl http://localhost:$PORT/"
    target: "../next"
    services:
      - command: "my-server --port 0"
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.transitions[0].services.len(), 1);
        assert_eq!(
            config.transitions[0].services[0].command,
            "my-server --port 0"
        );
        assert!(config.transitions[0].services[0].shell);
        assert!(config.transitions[0].services[0].port_pattern.is_none());
        assert!(config.transitions[0].services[0].ready.is_none());
    }

    #[test]
    fn parse_transition_with_services_full() {
        let yaml = r#"
transitions:
  - command: "curl http://localhost:$PORT/"
    target: "../next"
    services:
      - command: "/usr/bin/my-server"
        shell: false
        port_pattern: "Serving on port (\\d+)"
        ready: "curl -sf http://localhost:$PORT/health"
"#;
        let config = parse_config(yaml).unwrap();
        let svc = &config.transitions[0].services[0];
        assert_eq!(svc.command, "/usr/bin/my-server");
        assert!(!svc.shell);
        assert_eq!(svc.port_pattern.as_deref(), Some("Serving on port (\\d+)"));
        assert_eq!(
            svc.ready.as_deref(),
            Some("curl -sf http://localhost:$PORT/health")
        );
    }

    #[test]
    fn parse_transition_services_absent() {
        let yaml = r#"
transitions:
  - command: "echo hi"
    target: "../next"
"#;
        let config = parse_config(yaml).unwrap();
        assert!(config.transitions[0].services.is_empty());
    }

    #[test]
    fn parse_assertion_with_services() {
        let yaml = r#"
assertions:
  - command: "curl -sf http://localhost:$PORT/"
    services:
      - command: "my-server --port 0"
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.assertions[0].services.len(), 1);
        assert_eq!(
            config.assertions[0].services[0].command,
            "my-server --port 0"
        );
    }

    #[test]
    fn parse_assertion_services_absent() {
        let yaml = r#"
assertions:
  - command: "echo hi"
"#;
        let config = parse_config(yaml).unwrap();
        assert!(config.assertions[0].services.is_empty());
    }

    #[test]
    fn parse_multiple_services() {
        let yaml = r#"
transitions:
  - command: "test-both"
    target: "../next"
    services:
      - command: "server-a --port 0"
      - command: "server-b --port 0"
        ready: "curl -sf http://localhost:$PORT_1/ready"
"#;
        let config = parse_config(yaml).unwrap();
        assert_eq!(config.transitions[0].services.len(), 2);
        assert_eq!(
            config.transitions[0].services[0].command,
            "server-a --port 0"
        );
        assert_eq!(
            config.transitions[0].services[1].command,
            "server-b --port 0"
        );
        assert!(config.transitions[0].services[1].ready.is_some());
    }

    #[test]
    fn parse_state_config_doc_field() {
        let yaml = r#"
doc: |
  This state represents an initialized repository.
  It has a clean working tree.
"#;
        let config = parse_config(yaml).unwrap();
        let doc = config.doc.as_deref().unwrap();
        assert!(doc.contains("initialized repository"));
        assert!(doc.contains("clean working tree"));
    }

    #[test]
    fn parse_transition_doc_field() {
        let yaml = r#"
transitions:
  - name: "tisket init"
    command: "tisket init"
    target: "../initialized"
    doc: |
      The generated tisket.yml configures where issues are stored.
"#;
        let config = parse_config(yaml).unwrap();
        let doc = config.transitions[0].doc.as_deref().unwrap();
        assert!(doc.contains("tisket.yml configures"));
    }

    #[test]
    fn parse_config_doc_absent_defaults_to_none() {
        let yaml = "transitions: []";
        let config = parse_config(yaml).unwrap();
        assert!(config.doc.is_none());
    }

    #[test]
    fn parse_transition_doc_absent_defaults_to_none() {
        let yaml = r#"
transitions:
  - command: "echo hi"
    target: "../next"
"#;
        let config = parse_config(yaml).unwrap();
        assert!(config.transitions[0].doc.is_none());
    }
}
