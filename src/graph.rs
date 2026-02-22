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
}

impl StateGraph {
    /// Discover all states under `root` and build the graph.
    /// `config_dir` is the name of the config directory (e.g., ".missouri").
    pub fn discover(root: &Utf8Path, config_dir: &str) -> Result<Self> {
        let root = root.canonicalize_utf8().map_err(|e| Error::Io(e))?;

        // Phase 1: Find all directories containing <config_dir>/missouri.yml
        let mut state_paths: Vec<Utf8PathBuf> = Vec::new();
        collect_states(&root, config_dir, &mut state_paths)?;
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

            path_to_id.insert(path.clone(), id);
            states.push(State {
                id,
                path: path.clone(),
                name,
                env: cfg.env.clone(),
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
                });
            }
        }

        let ignore = load_ignore_patterns(&root, config_dir)?;

        Ok(StateGraph {
            states,
            transitions,
            adjacency,
            assertions,
            config_dir: config_dir.to_string(),
            root: root.clone(),
            ignore,
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

/// Recursively find directories containing `<config_dir>/missouri.yml`.
fn collect_states(dir: &Utf8Path, config_dir: &str, out: &mut Vec<Utf8PathBuf>) -> Result<()> {
    let cfg_dir = dir.join(config_dir);
    let config_file = cfg_dir.join("missouri.yml");
    if config_file.exists() {
        out.push(dir.to_owned());
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
            collect_states(&path, config_dir, out)?;
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
}
