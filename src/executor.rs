use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use crate::compare::{self, ComparisonResult};
use crate::error;
use crate::graph::{StateGraph, StateId, Transition};
use crate::paths::TestPath;

/// Sandbox configuration for transition execution.
#[derive(Debug, Clone)]
pub enum Sandbox {
    /// No sandbox — env_clear + manual PATH construction.
    None,
    /// Flox environment at the project root.
    Flox {
        /// Absolute path to the `flox` binary (resolved from user's PATH at startup).
        flox_bin: Utf8PathBuf,
        /// Absolute path to the directory containing `.flox/`.
        project_root: Utf8PathBuf,
    },
}

/// Detect sandbox from the project root.
///
/// If `.flox/` exists at the root, resolve the `flox` binary from the current
/// process's PATH and return `Sandbox::Flox`. Errors if `.flox/` is present
/// but `flox` is not found.
pub fn detect_sandbox(graph: &StateGraph) -> error::Result<Sandbox> {
    let flox_dir = graph.root.join(".flox");
    if !flox_dir.exists() {
        return Ok(Sandbox::None);
    }

    let flox_bin = which_flox().ok_or_else(|| error::Error::FloxNotFound {
        root: graph.root.clone(),
    })?;

    Ok(Sandbox::Flox {
        flox_bin,
        project_root: graph.root.clone(),
    })
}

/// Resolve the absolute path to `flox` from the current process's PATH.
fn which_flox() -> Option<Utf8PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Utf8PathBuf::from(dir).join("flox");
        if candidate.exists() {
            return Some(candidate);
        }
    }
    std::option::Option::None
}

/// Result of executing a single transition.
#[derive(Debug)]
pub struct StepResult {
    pub transition_name: String,
    pub source_name: String,
    pub target_name: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub comparison: Option<ComparisonResult>,
    pub passed: bool,
}

/// Result of executing a full test path.
#[derive(Debug)]
pub struct PathResult {
    pub path_display: String,
    pub steps: Vec<StepResult>,
    pub passed: bool,
}

/// Options for test execution.
pub struct RunOptions {
    pub keep_temp: bool,
    pub verbose: bool,
    pub sandbox: Sandbox,
}

/// Execute all test paths and return results.
pub fn run_all_paths(graph: &StateGraph, paths: &[TestPath], opts: &RunOptions) -> Vec<PathResult> {
    paths
        .iter()
        .map(|path| run_path(graph, path, opts))
        .collect()
}

/// Execute a single test path.
fn run_path(graph: &StateGraph, path: &TestPath, opts: &RunOptions) -> PathResult {
    let path_display = path.display(graph);
    let mut steps = Vec::new();
    let mut passed = true;

    // For chained paths (A → B → C), the output of one transition
    // becomes the input for the next. Start with the first state.
    let mut current_dir: Option<(TempDir, Utf8PathBuf)> = None;

    for (step_idx, &transition_idx) in path.steps.iter().enumerate() {
        let transition = &graph.transitions[transition_idx];
        let source = &graph.states[transition.source.0];
        let target = &graph.states[transition.target.0];

        // Determine the working directory for this step.
        // First step: copy the source state to a temp dir.
        // Subsequent steps: use the temp dir from the previous step.
        let (temp_dir, work_dir) = if step_idx == 0 {
            match copy_state_to_temp(source.id, graph) {
                Ok(pair) => pair,
                Err(e) => {
                    steps.push(StepResult {
                        transition_name: transition.name.clone(),
                        source_name: source.name.clone(),
                        target_name: target.name.clone(),
                        exit_code: None,
                        stdout: String::new(),
                        stderr: e,
                        comparison: None,
                        passed: false,
                    });
                    passed = false;
                    break;
                }
            }
        } else {
            match current_dir.take() {
                Some(pair) => pair,
                None => match copy_state_to_temp(source.id, graph) {
                    Ok(pair) => pair,
                    Err(e) => {
                        steps.push(StepResult {
                            transition_name: transition.name.clone(),
                            source_name: source.name.clone(),
                            target_name: target.name.clone(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e,
                            comparison: None,
                            passed: false,
                        });
                        passed = false;
                        break;
                    }
                },
            }
        };

        // Execute the transition command in the sandboxed env
        let step_result = execute_transition(
            transition,
            &work_dir,
            &source.env,
            target,
            graph,
            &opts.sandbox,
        );

        let step_passed = step_result.passed;
        if !step_passed {
            passed = false;
        }

        // If this step passed and there are more steps, carry the temp dir forward
        if step_passed && step_idx + 1 < path.steps.len() {
            current_dir = Some((temp_dir, work_dir));
        } else if !opts.keep_temp {
            drop(temp_dir); // cleanup
        }

        steps.push(step_result);

        if !step_passed {
            break; // stop on first failure
        }
    }

    PathResult {
        path_display,
        steps,
        passed,
    }
}

/// Copy a state's files (excluding .missouri/) to a temp directory.
fn copy_state_to_temp(
    state_id: StateId,
    graph: &StateGraph,
) -> std::result::Result<(TempDir, Utf8PathBuf), String> {
    let state = &graph.states[state_id.0];
    let temp_dir = TempDir::new().map_err(|e| format!("failed to create temp dir: {e}"))?;
    let temp_path = Utf8PathBuf::try_from(temp_dir.path().to_owned())
        .map_err(|e| format!("temp dir path not UTF-8: {e}"))?;

    copy_dir_recursive(&state.path, &temp_path, &graph.config_dir)
        .map_err(|e| format!("failed to copy state to temp dir: {e}"))?;

    Ok((temp_dir, temp_path))
}

/// Recursively copy directory contents, skipping the config directory.
fn copy_dir_recursive(src: &Utf8Path, dst: &Utf8Path, config_dir: &str) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str == config_dir {
            continue;
        }

        let src_path = Utf8PathBuf::try_from(entry.path())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let dst_path = dst.join(name_str.as_ref());

        let ft = entry.file_type()?;
        if ft.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path, config_dir)?;
        } else if ft.is_symlink() {
            let target = std::fs::read_link(&src_path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&target, &dst_path)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&target, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Execute a single transition command and compare the result.
fn execute_transition(
    transition: &Transition,
    work_dir: &Utf8Path,
    source_env: &std::collections::BTreeMap<String, String>,
    target: &crate::graph::State,
    graph: &StateGraph,
    sandbox: &Sandbox,
) -> StepResult {
    let source_name = graph.states[transition.source.0].name.clone();
    let target_name = target.name.clone();

    // Build PATH: source state's config bin/ prepended to the state's PATH (or system defaults)
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let base_path = source_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or("/usr/local/bin:/usr/bin:/bin");
    let path_env = if bin_dir.exists() {
        format!("{}:{}", bin_dir, base_path)
    } else {
        base_path.to_string()
    };

    // Run the command, wrapping in flox activate when a Flox sandbox is active.
    let output = match sandbox {
        Sandbox::None => build_command_bare(transition, work_dir, source_env, &path_env),
        Sandbox::Flox {
            flox_bin,
            project_root,
        } => build_command_flox(
            transition,
            work_dir,
            source_env,
            &path_env,
            flox_bin,
            project_root,
        ),
    };

    // Handle empty command (non-shell mode)
    let output = match output {
        Some(result) => result,
        None => {
            return StepResult {
                transition_name: transition.name.clone(),
                source_name,
                target_name,
                exit_code: None,
                stdout: String::new(),
                stderr: "empty command".into(),
                comparison: None,
                passed: false,
            };
        }
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return StepResult {
                transition_name: transition.name.clone(),
                source_name,
                target_name,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("failed to execute command: {e}"),
                comparison: None,
                passed: false,
            };
        }
    };

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    if !output.status.success() {
        return StepResult {
            transition_name: transition.name.clone(),
            source_name,
            target_name,
            exit_code,
            stdout,
            stderr,
            comparison: None,
            passed: false,
        };
    }

    // Resolve bin_dir from the source state's config dir bin/
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let bin_dir_ref = if bin_dir.exists() {
        Some(bin_dir.as_path())
    } else {
        None
    };

    // Extract flox paths for comparator execution
    let flox = match sandbox {
        Sandbox::Flox {
            flox_bin,
            project_root,
        } => Some((flox_bin.as_path(), project_root.as_path())),
        Sandbox::None => None,
    };

    // Compare the result against the expected target state
    let comparison = compare::compare_trees(
        work_dir,
        &target.path,
        &transition.file_comparators,
        bin_dir_ref,
        &graph.config_dir,
        &graph.ignore,
        flox,
    );

    // Compare env vars only when the target state or transition defines env expectations.
    // If neither the target has env vars nor the transition has env comparators, skip.
    let env_diffs = if !target.env.is_empty() || !transition.env_comparators.is_empty() {
        compare::compare_env(
            &source_env,
            &target.env,
            &transition.env_comparators,
            bin_dir_ref,
            flox,
        )
    } else {
        Vec::new()
    };

    let mut comparison = comparison;
    comparison.env_diffs = env_diffs;
    comparison.passed = comparison.passed && comparison.env_diffs.is_empty();

    let passed = comparison.passed;

    StepResult {
        transition_name: transition.name.clone(),
        source_name,
        target_name,
        exit_code,
        stdout,
        stderr,
        comparison: Some(comparison),
        passed,
    }
}

/// Build a command without any sandbox wrapping (env_clear + manual PATH).
fn build_command_bare(
    transition: &Transition,
    work_dir: &Utf8Path,
    source_env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
) -> Option<std::io::Result<std::process::Output>> {
    if transition.shell {
        Some(
            Command::new("sh")
                .arg("-c")
                .arg(&transition.command)
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(source_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    } else {
        let parts: Vec<&str> = transition.command.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        Some(
            Command::new(parts[0])
                .args(&parts[1..])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(source_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    }
}

/// Build a command wrapped in `flox activate` (env_clear + state vars + PATH).
///
/// Flox uses `-- <cmd>` for all command execution (no `-c` flag).
/// Shell mode: `flox activate -d <root> -- sh -c "<command>"`
/// Non-shell:  `flox activate -d <root> -- <cmd> <args...>`
fn build_command_flox(
    transition: &Transition,
    work_dir: &Utf8Path,
    source_env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
    flox_bin: &Utf8Path,
    project_root: &Utf8Path,
) -> Option<std::io::Result<std::process::Output>> {
    if transition.shell {
        Some(
            Command::new(flox_bin.as_str())
                .args([
                    "activate",
                    "-d",
                    project_root.as_str(),
                    "--",
                    "sh",
                    "-c",
                    &transition.command,
                ])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(source_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    } else {
        let parts: Vec<&str> = transition.command.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        let mut args = vec!["activate", "-d", project_root.as_str(), "--"];
        args.extend(parts);
        Some(
            Command::new(flox_bin.as_str())
                .args(&args)
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(source_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use camino::Utf8Path;
    use std::fs;

    fn make_state(tmp: &Utf8Path, name: &str, yaml: &str) {
        let state_dir = tmp.join(name);
        let missouri_dir = state_dir.join(".missouri");
        fs::create_dir_all(&missouri_dir).unwrap();
        fs::write(missouri_dir.join("missouri.yml"), yaml).unwrap();
    }

    #[test]
    fn detect_sandbox_none_without_flox_dir() {
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
        let sandbox = detect_sandbox(&graph).unwrap();
        assert!(matches!(sandbox, Sandbox::None));
    }

    #[test]
    fn detect_sandbox_flox_when_dir_exists() {
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

        // Create a .flox/ directory at the project root
        fs::create_dir_all(root.join(".flox")).unwrap();

        let graph = StateGraph::discover(root, ".missouri").unwrap();
        let sandbox = detect_sandbox(&graph).unwrap();

        // flox is installed on this machine, so it should resolve
        match sandbox {
            Sandbox::Flox {
                flox_bin,
                project_root,
            } => {
                assert!(flox_bin.as_str().contains("flox"));
                assert_eq!(project_root, graph.root);
            }
            Sandbox::None => panic!("expected Sandbox::Flox"),
        }
    }

    #[test]
    fn which_flox_finds_binary() {
        // flox is installed on this system
        let result = which_flox();
        assert!(result.is_some());
        assert!(result.unwrap().as_str().ends_with("flox"));
    }
}
