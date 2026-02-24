use std::collections::{BTreeMap, BTreeSet, HashMap};

use camino::{Utf8Path, Utf8PathBuf};
use ignore::gitignore::Gitignore;

use crate::config::{self, StateConfig, TransitionConfig};
use crate::error::{Error, Result};

/// Opaque index into the graph's state list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StateId(pub usize);

/// A resolved state node in the graph.
#[derive(Debug)]
pub struct State {
    pub id: StateId,
    /// Absolute path to the state directory.
    pub path: Utf8PathBuf,
    /// Human-readable name (directory basename).
    pub name: String,
    /// Environment variables defined for this state.
    pub env: BTreeMap<String, String>,
}

/// A resolved file comparator override.
#[derive(Debug, Clone)]
pub enum FileComparator {
    Ignore,
    Custom { command: String },
}

/// A resolved env comparator override.
#[derive(Debug, Clone)]
pub enum EnvComparator {
    Ignore,
    Custom { command: String },
}

/// A resolved transition (edge) in the graph.
#[derive(Debug)]
pub struct Transition {
    pub name: String,
    pub command: String,
    pub shell: bool,
    pub source: StateId,
    pub target: StateId,
    /// File comparison overrides: path → comparator.
    pub file_comparators: Vec<(Utf8PathBuf, FileComparator)>,
    /// Env var comparison overrides: var name → comparator.
    pub env_comparators: Vec<(String, EnvComparator)>,
    /// Expected stdout (exact match) when assertions are enabled.
    pub expected_stdout: Option<String>,
    /// Expected stderr (exact match) when assertions are enabled.
    pub expected_stderr: Option<String>,
}

/// A resolved assertion attached to a state.
#[derive(Debug)]
pub struct Assertion {
    pub name: String,
    pub command: String,
    pub shell: bool,
    pub state: StateId,
    pub expected_stdout: Option<String>,
    pub expected_stderr: Option<String>,
    pub should_fail: bool,
}

/// A resolved setup command from project-level config.
#[derive(Debug)]
pub struct SetupCommand {
    pub name: String,
    pub command: String,
    pub shell: bool,
}

/// Sandbox configuration parsed from project-level missouri.yml.
#[derive(Debug, Clone)]
pub enum SandboxConfig {
    /// No sandbox — bare execution with env_clear + manual PATH.
    None,
    /// Simple mode: install these nix packages via flox.
    Packages(Vec<String>),
    /// Advanced mode: use a user-provided manifest.toml.
    Manifest(Utf8PathBuf),
}

/// The complete state graph.
#[derive(Debug)]
pub struct StateGraph {
    pub states: Vec<State>,
    pub transitions: Vec<Transition>,
    /// Adjacency list: state → outgoing transition indices.
    pub adjacency: HashMap<StateId, Vec<usize>>,
    /// Assertions attached to states.
    pub assertions: Vec<Assertion>,
    /// Name of the config directory (e.g., ".missouri").
    pub config_dir: String,
    /// Absolute path to the project root directory.
    pub root: Utf8PathBuf,
    /// Project-level ignore patterns from `<config_dir>/ignore` (gitignore syntax).
    pub ignore: Gitignore,
    /// Project-level env vars (before state-level merging).
    pub project_env: std::collections::BTreeMap<String, String>,
    /// Project-level setup commands (run before test paths).
    pub setup: Vec<SetupCommand>,
    /// Project-level shared bin/ directory (if it exists).
    pub project_bin: Option<Utf8PathBuf>,
    /// Sandbox configuration from project-level missouri.yml.
    pub sandbox_config: SandboxConfig,
}

impl StateGraph {
    /// Discover all states under `root` and build the graph.
    /// `config_dir` is the name of the config directory (e.g., ".missouri").
    ///
    /// Config is loaded from one of two locations (checked in order):
    /// 1. `<root>/missouri.yml` — root-level config, may have `test_dir` pointing elsewhere
    /// 2. `<root>/<config_dir>/missouri.yml` — config-dir-level config (original behavior)
    pub fn discover(root: &Utf8Path, config_dir: &str) -> Result<Self> {
        let root = root.canonicalize_utf8().map_err(|e| Error::Io(e))?;

        // Phase 0: Load project-level config
        // Check root-level missouri.yml first, then fall back to <config_dir>/missouri.yml
        let (project_env, setup, project_bin, sandbox_config, state_root) =
            load_project_config(&root, config_dir)?;

        // Phase 1: Find all directories containing <config_dir>/missouri.yml
        // (excludes the state root itself — root config is project-level, not a state)
        let mut state_paths: Vec<Utf8PathBuf> = Vec::new();
        collect_states(&state_root, config_dir, &state_root, &mut state_paths)?;
        state_paths.sort();

        // Phase 2: Build state nodes
        let mut path_to_id: HashMap<Utf8PathBuf, StateId> = HashMap::new();
        let mut states: Vec<State> = Vec::new();
        let mut configs: Vec<StateConfig> = Vec::new();

        for (i, path) in state_paths.iter().enumerate() {
            let id = StateId(i);
            let config_path = path.join(config_dir).join("missouri.yml");
            let content = std::fs::read_to_string(&config_path).map_err(|e| Error::ConfigRead {
                path: config_path.clone(),
                source: e,
            })?;
            let cfg = config::parse_config(&content).map_err(|e| Error::ConfigParse {
                path: config_path,
                source: e,
            })?;

            let name = path.file_name().unwrap_or(path.as_str()).to_string();

            // Merge env: project env is the base, state env overrides
            let mut merged_env = project_env.clone();
            merged_env.extend(cfg.env.iter().map(|(k, v)| (k.clone(), v.clone())));

            path_to_id.insert(path.clone(), id);
            states.push(State {
                id,
                path: path.clone(),
                name,
                env: merged_env,
            });
            configs.push(cfg);
        }

        // Phase 3: Resolve transitions
        let mut transitions: Vec<Transition> = Vec::new();
        let mut adjacency: HashMap<StateId, Vec<usize>> = HashMap::new();

        for (i, cfg) in configs.iter().enumerate() {
            let source_id = StateId(i);
            let source_path = &states[i].path;

            for (t_idx, t) in cfg.transitions.iter().enumerate() {
                let target_abs = source_path
                    .join(&t.target)
                    .canonicalize_utf8()
                    .map_err(|_| Error::MissingTarget {
                        from_state: source_path.clone(),
                        target: t.target.clone(),
                    })?;

                let target_id =
                    path_to_id
                        .get(&target_abs)
                        .ok_or_else(|| Error::MissingTarget {
                            from_state: source_path.clone(),
                            target: t.target.clone(),
                        })?;

                let name = t
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}[{}]", states[i].name, t_idx));

                let file_comparators = resolve_file_comparators(t);
                let env_comparators = resolve_env_comparators(t);

                let transition_idx = transitions.len();
                transitions.push(Transition {
                    name,
                    command: t.command.clone(),
                    shell: t.shell,
                    source: source_id,
                    target: *target_id,
                    file_comparators,
                    env_comparators,
                    expected_stdout: t.stdout.clone(),
                    expected_stderr: t.stderr.clone(),
                });

                adjacency.entry(source_id).or_default().push(transition_idx);
            }
        }

        // Phase 4: Resolve assertions
        let mut assertions: Vec<Assertion> = Vec::new();
        for (i, cfg) in configs.iter().enumerate() {
            let state_id = StateId(i);
            for (a_idx, a) in cfg.assertions.iter().enumerate() {
                let name = a
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("{}:assert[{}]", states[i].name, a_idx));
                assertions.push(Assertion {
                    name,
                    command: a.command.clone(),
                    shell: a.shell,
                    state: state_id,
                    expected_stdout: a.stdout.clone(),
                    expected_stderr: a.stderr.clone(),
                    should_fail: a.should_fail,
                });
            }
        }

        let ignore = load_ignore_patterns(&state_root, config_dir)?;

        Ok(StateGraph {
            states,
            transitions,
            adjacency,
            assertions,
            config_dir: config_dir.to_string(),
            root: state_root,
            ignore,
            project_env,
            setup,
            project_bin,
            sandbox_config,
        })
    }

    /// Find root states (no inbound transitions).
    pub fn roots(&self) -> Vec<StateId> {
        let mut has_inbound: BTreeSet<StateId> = BTreeSet::new();
        for t in &self.transitions {
            has_inbound.insert(t.target);
        }
        self.states
            .iter()
            .filter(|s| !has_inbound.contains(&s.id))
            .map(|s| s.id)
            .collect()
    }

    /// Get outgoing transitions for a state.
    pub fn outgoing(&self, state: StateId) -> &[usize] {
        self.adjacency
            .get(&state)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get assertions for a state.
    pub fn assertions_for(&self, state: StateId) -> Vec<&Assertion> {
        self.assertions
            .iter()
            .filter(|a| a.state == state)
            .collect()
    }
}

/// Load ignore patterns from `<root>/<config_dir>/ignore`.
///
/// Uses gitignore syntax: trailing `/` matches directories, `!` negates,
/// `**` matches across directories, `#` for comments.
fn load_ignore_patterns(root: &Utf8Path, config_dir: &str) -> Result<Gitignore> {
    let ignore_path = root.join(config_dir).join("ignore");
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);

    if ignore_path.exists() {
        if let Some(err) = builder.add(&ignore_path) {
            return Err(Error::IgnorePattern {
                pattern: ignore_path.to_string(),
                detail: err.to_string(),
            });
        }
    }

    builder.build().map_err(|e| Error::IgnorePattern {
        pattern: ignore_path.to_string(),
        detail: e.to_string(),
    })
}

/// Load project-level config, checking two locations in order:
/// 1. `<root>/missouri.yml` — root-level config (may include `test_dir`)
/// 2. `<root>/<config_dir>/missouri.yml` — config-dir-level config
///
/// Returns (project_env, setup_commands, project_bin, sandbox_config, state_root).
/// `state_root` is the directory where state discovery should start (may differ
/// from `root` if `test_dir` is set in a root-level missouri.yml).
fn load_project_config(
    root: &Utf8Path,
    config_dir: &str,
) -> Result<(
    BTreeMap<String, String>,
    Vec<SetupCommand>,
    Option<Utf8PathBuf>,
    SandboxConfig,
    Utf8PathBuf,
)> {
    let root_yml = root.join("missouri.yml");
    let config_dir_yml = root.join(config_dir).join("missouri.yml");

    let (config_path, cfg) = if root_yml.exists() {
        let content = std::fs::read_to_string(&root_yml).map_err(|e| Error::ConfigRead {
            path: root_yml.clone(),
            source: e,
        })?;
        let cfg = config::parse_project_config(&content).map_err(|e| Error::ConfigParse {
            path: root_yml.clone(),
            source: e,
        })?;
        (root_yml, Some(cfg))
    } else if config_dir_yml.exists() {
        let content = std::fs::read_to_string(&config_dir_yml).map_err(|e| Error::ConfigRead {
            path: config_dir_yml.clone(),
            source: e,
        })?;
        let cfg = config::parse_project_config(&content).map_err(|e| Error::ConfigParse {
            path: config_dir_yml.clone(),
            source: e,
        })?;
        (config_dir_yml, Some(cfg))
    } else {
        (config_dir_yml, None)
    };

    let (project_env, setup, sandbox_config, state_root) = if let Some(cfg) = cfg {
        let setup_commands: Vec<SetupCommand> = cfg
            .setup
            .iter()
            .enumerate()
            .map(|(i, s)| SetupCommand {
                name: s.name.clone().unwrap_or_else(|| format!("setup[{i}]")),
                command: s.command.clone(),
                shell: s.shell,
            })
            .collect();

        let sandbox = if let Some(flox_cfg) = cfg.flox {
            SandboxConfig::Manifest(root.join(&flox_cfg.manifest))
        } else if !cfg.packages.is_empty() {
            SandboxConfig::Packages(cfg.packages)
        } else {
            SandboxConfig::None
        };

        // If test_dir is set, resolve it relative to root
        let state_root = if let Some(test_dir) = cfg.test_dir {
            root.join(&test_dir)
                .canonicalize_utf8()
                .map_err(|_| Error::ConfigRead {
                    path: config_path,
                    source: std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("test_dir not found: {test_dir}"),
                    ),
                })?
        } else {
            root.to_owned()
        };

        (cfg.env, setup_commands, sandbox, state_root)
    } else {
        (
            BTreeMap::new(),
            Vec::new(),
            SandboxConfig::None,
            root.to_owned(),
        )
    };

    // Look for bin/ in both the state_root's config_dir and the root's config_dir
    let bin_path = state_root.join(config_dir).join("bin");
    let project_bin = if bin_path.exists() {
        Some(bin_path)
    } else {
        let root_bin = root.join(config_dir).join("bin");
        if root_bin.exists() {
            Some(root_bin)
        } else {
            None
        }
    };

    Ok((project_env, setup, project_bin, sandbox_config, state_root))
}

/// Recursively find directories containing `<config_dir>/missouri.yml`.
/// Skips the project root (its missouri.yml is project-level config, not a state).
fn collect_states(
    dir: &Utf8Path,
    config_dir: &str,
    project_root: &Utf8Path,
    out: &mut Vec<Utf8PathBuf>,
) -> Result<()> {
    // Don't treat the project root as a state
    if dir != project_root {
        let cfg_dir = dir.join(config_dir);
        let config_file = cfg_dir.join("missouri.yml");
        if config_file.exists() {
            out.push(dir.to_owned());
        }
    }

    let entries = std::fs::read_dir(dir)?;
    for entry in entries {
        let entry = entry?;
        let path = Utf8PathBuf::try_from(entry.path())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        if entry.file_type()?.is_dir() {
            let name = path.file_name().unwrap_or("");
            // Skip hidden dirs (except we already checked config dir above)
            if name.starts_with('.') {
                continue;
            }
            collect_states(&path, config_dir, project_root, out)?;
        }
    }
    Ok(())
}

fn resolve_file_comparators(t: &TransitionConfig) -> Vec<(Utf8PathBuf, FileComparator)> {
    let Some(comps) = &t.comparators else {
        return vec![];
    };
    comps
        .files
        .iter()
        .map(|fc| {
            let comparator = if fc.ignore {
                FileComparator::Ignore
            } else if let Some(cmd) = &fc.command {
                FileComparator::Custom {
                    command: cmd.clone(),
                }
            } else {
                // No override specified — shouldn't appear, but treat as no-op
                return (
                    fc.path.clone(),
                    FileComparator::Custom {
                        command: String::new(),
                    },
                );
            };
            (fc.path.clone(), comparator)
        })
        .collect()
}

fn resolve_env_comparators(t: &TransitionConfig) -> Vec<(String, EnvComparator)> {
    let Some(comps) = &t.comparators else {
        return vec![];
    };
    comps
        .env
        .iter()
        .map(|ec| {
            let comparator = if ec.ignore {
                EnvComparator::Ignore
            } else if let Some(cmd) = &ec.command {
                EnvComparator::Custom {
                    command: cmd.clone(),
                }
            } else {
                return (
                    ec.name.clone(),
                    EnvComparator::Custom {
                        command: String::new(),
                    },
                );
            };
            (ec.name.clone(), comparator)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_state(tmp: &Utf8Path, name: &str, yaml: &str) {
        let state_dir = tmp.join(name);
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), yaml).unwrap();
    }

    #[test]
    fn discover_trivial() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo hi"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.transitions.len(), 1);

        let roots = graph.roots();
        assert_eq!(roots.len(), 1);
        assert_eq!(graph.states[roots[0].0].name, "a");
    }

    #[test]
    fn discover_cycle_no_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(
            root,
            "b",
            r#"
transitions:
  - command: "echo"
    target: "../a"
"#,
        );

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.transitions.len(), 2);
        assert!(graph.roots().is_empty());
    }

    #[test]
    fn discover_branching() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "start",
            r#"
transitions:
  - name: "left"
    command: "echo left"
    target: "../left"
  - name: "right"
    command: "echo right"
    target: "../right"
"#,
        );
        make_state(root, "left", "{}");
        make_state(root, "right", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 3);
        assert_eq!(graph.transitions.len(), 2);

        let roots = graph.roots();
        assert_eq!(roots.len(), 1);

        let outgoing = graph.outgoing(roots[0]);
        assert_eq!(outgoing.len(), 2);
    }

    #[test]
    fn discover_project_config_env_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Project-level config with env
        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            r#"
env:
  PROJECT_VAR: from_project
  OVERRIDE_ME: project_value
"#,
        )
        .unwrap();

        // State with its own env that overrides one key
        make_state(
            root,
            "a",
            r#"
env:
  OVERRIDE_ME: state_value
  STATE_VAR: only_in_state
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();

        // Find state "a" — should have merged env
        let state_a = graph.states.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(state_a.env["PROJECT_VAR"], "from_project");
        assert_eq!(state_a.env["OVERRIDE_ME"], "state_value");
        assert_eq!(state_a.env["STATE_VAR"], "only_in_state");

        // State "b" has no state env — should inherit project env
        let state_b = graph.states.iter().find(|s| s.name == "b").unwrap();
        assert_eq!(state_b.env["PROJECT_VAR"], "from_project");
        assert_eq!(state_b.env["OVERRIDE_ME"], "project_value");
    }

    #[test]
    fn discover_project_bin_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let root_missouri = root.join(".missouri");
        let root_bin = root_missouri.join("bin");
        fs::create_dir_all(&root_bin).unwrap();
        fs::write(root_missouri.join("missouri.yml"), "{}").unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(graph.project_bin.is_some());
        assert!(graph
            .project_bin
            .unwrap()
            .as_str()
            .ends_with(".missouri/bin"));
    }

    #[test]
    fn discover_project_bin_none_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(graph.project_bin.is_none());
    }

    #[test]
    fn discover_setup_commands() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            r#"
setup:
  - name: "build"
    command: "cargo build"
  - command: "db-seed"
    shell: false
"#,
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.setup.len(), 2);
        assert_eq!(graph.setup[0].name, "build");
        assert_eq!(graph.setup[0].command, "cargo build");
        assert!(graph.setup[0].shell);
        assert_eq!(graph.setup[1].command, "db-seed");
        assert!(!graph.setup[1].shell);
    }

    #[test]
    fn discover_no_project_config_is_fine() {
        // No root-level missouri.yml — everything should still work
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(graph.setup.is_empty());
        assert!(graph.project_bin.is_none());
    }

    #[test]
    fn discover_root_level_missouri_yml() {
        // Root-level missouri.yml (no test_dir) — project config lives at root
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        fs::write(
            root.join("missouri.yml"),
            r#"
env:
  FROM_ROOT: "yes"
"#,
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.project_env["FROM_ROOT"], "yes");
        // States should inherit the root-level env
        let state_a = graph.states.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(state_a.env["FROM_ROOT"], "yes");
    }

    #[test]
    fn discover_root_level_overrides_config_dir() {
        // Both root-level missouri.yml and .missouri/missouri.yml exist.
        // Root-level should win.
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        fs::write(
            root.join("missouri.yml"),
            r#"
env:
  SOURCE: root_level
"#,
        )
        .unwrap();

        let config_dir = root.join(".missouri");
        fs::create_dir_all(&config_dir).unwrap();
        fs::write(
            config_dir.join("missouri.yml"),
            r#"
env:
  SOURCE: config_dir_level
"#,
        )
        .unwrap();

        make_state(
            root,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(root, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        // Root-level missouri.yml takes precedence
        assert_eq!(graph.project_env["SOURCE"], "root_level");
    }

    #[test]
    fn discover_test_dir_redirects_state_discovery() {
        // Root has missouri.yml with test_dir pointing to a subdirectory.
        // States live in that subdirectory, not the root.
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("tests").join("smoke");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(
            root.join("missouri.yml"),
            r#"
test_dir: tests/smoke
env:
  FROM_ROOT: "yes"
"#,
        )
        .unwrap();

        // States live under tests/smoke/
        make_state(
            &tests_dir,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.project_env["FROM_ROOT"], "yes");
        // graph.root should be the resolved test_dir, not the original root
        assert!(
            graph.root.as_str().ends_with("tests/smoke"),
            "graph.root should be the test_dir: {}",
            graph.root
        );
    }

    #[test]
    fn discover_test_dir_with_config_dir_ignore() {
        // test_dir has its own .missouri/ignore that should be used
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(root.join("missouri.yml"), "test_dir: tests").unwrap();

        // Create .missouri/ignore in the test_dir
        let test_config = tests_dir.join(".missouri");
        fs::create_dir_all(&test_config).unwrap();
        fs::write(test_config.join("ignore"), "*.log\n").unwrap();

        make_state(
            &tests_dir,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.states.len(), 2);
        // Verify that ignore patterns from test_dir are loaded.
        // The gitignore root is the canonicalized state_root, so use a
        // path relative to graph.root for the match check.
        let check_path = graph.root.join("a/foo.log");
        let matched = graph
            .ignore
            .matched_path_or_any_parents(check_path.as_std_path(), false);
        assert!(matched.is_ignore(), "*.log should be ignored");
    }

    #[test]
    fn discover_test_dir_missing_errors() {
        // test_dir points to a nonexistent directory — should error
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        fs::write(root.join("missouri.yml"), "test_dir: nonexistent/path").unwrap();

        let result = StateGraph::discover(root, ".missouri");
        assert!(result.is_err());
    }

    #[test]
    fn discover_test_dir_env_merges_into_states() {
        // Root missouri.yml has env + test_dir. States in test_dir
        // should inherit the root-level env.
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("suite");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(
            root.join("missouri.yml"),
            r#"
test_dir: suite
env:
  PROJECT_VAR: from_project
"#,
        )
        .unwrap();

        make_state(
            &tests_dir,
            "a",
            r#"
env:
  STATE_VAR: from_state
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let state_a = graph.states.iter().find(|s| s.name == "a").unwrap();
        assert_eq!(state_a.env["PROJECT_VAR"], "from_project");
        assert_eq!(state_a.env["STATE_VAR"], "from_state");
    }

    #[test]
    fn discover_test_dir_setup_commands() {
        // Root missouri.yml has setup + test_dir. Setup should be loaded.
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(
            root.join("missouri.yml"),
            r#"
test_dir: tests
setup:
  - name: "init db"
    command: "db-init"
"#,
        )
        .unwrap();

        make_state(
            &tests_dir,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert_eq!(graph.setup.len(), 1);
        assert_eq!(graph.setup[0].name, "init db");
    }

    #[test]
    fn discover_test_dir_bin_in_test_dir() {
        // bin/ exists in test_dir's .missouri — should be found
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(root.join("missouri.yml"), "test_dir: tests").unwrap();

        let test_bin = tests_dir.join(".missouri").join("bin");
        fs::create_dir_all(&test_bin).unwrap();

        make_state(
            &tests_dir,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(graph.project_bin.is_some());
        assert!(
            graph
                .project_bin
                .as_ref()
                .unwrap()
                .as_str()
                .contains("tests/.missouri/bin"),
            "bin should be in test_dir: {:?}",
            graph.project_bin
        );
    }

    #[test]
    fn discover_test_dir_bin_falls_back_to_root() {
        // bin/ exists in root's .missouri but not in test_dir — should fall back
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        let tests_dir = root.join("tests");
        fs::create_dir_all(&tests_dir).unwrap();

        fs::write(root.join("missouri.yml"), "test_dir: tests").unwrap();

        let root_bin = root.join(".missouri").join("bin");
        fs::create_dir_all(&root_bin).unwrap();

        make_state(
            &tests_dir,
            "a",
            r#"
transitions:
  - command: "echo"
    target: "../b"
"#,
        );
        make_state(&tests_dir, "b", "{}");

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        assert!(graph.project_bin.is_some());
        // Should find root's bin, not test_dir's
        let bin_str = graph.project_bin.as_ref().unwrap().as_str();
        assert!(
            !bin_str.contains("tests/.missouri/bin"),
            "bin should be root's, not test_dir's: {bin_str}"
        );
        assert!(bin_str.ends_with(".missouri/bin"));
    }
}
