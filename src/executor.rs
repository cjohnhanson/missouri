use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use crate::compare::{self, ComparisonResult, OutputDiff};
use crate::error;
use crate::graph::{Assertion, StateGraph, StateId, Transition};
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

/// How assertions interact with the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckMode {
    /// Run transitions + filesystem comparison + output assertions + state assertions.
    Full,
    /// Run only state assertions (no transitions, no filesystem comparison).
    CheckOnly,
    /// Run transitions + filesystem comparison, skip all assertions.
    NoCheck,
}

/// Result of running a single state assertion.
#[derive(Debug)]
pub struct AssertionResult {
    pub name: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout_diff: Option<(String, String)>,
    pub stderr_diff: Option<(String, String)>,
    pub error: Option<String>,
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
    pub output_diffs: Vec<OutputDiff>,
    pub assertion_results: Vec<AssertionResult>,
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
    pub check_mode: CheckMode,
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

    match opts.check_mode {
        CheckMode::CheckOnly => run_path_check_only(graph, path, path_display, opts),
        CheckMode::Full | CheckMode::NoCheck => {
            run_path_transitions(graph, path, path_display, opts)
        }
    }
}

/// CheckOnly mode: iterate states in path order, run assertions on each.
fn run_path_check_only(
    graph: &StateGraph,
    path: &TestPath,
    path_display: String,
    opts: &RunOptions,
) -> PathResult {
    let mut steps = Vec::new();
    let mut passed = true;

    // Collect states in path order (source of first, then targets)
    let mut state_ids: Vec<StateId> = Vec::new();
    if let Some(&first_ti) = path.steps.first() {
        state_ids.push(graph.transitions[first_ti].source);
    }
    for &ti in &path.steps {
        state_ids.push(graph.transitions[ti].target);
    }

    for (i, &state_id) in state_ids.iter().enumerate() {
        let state = &graph.states[state_id.0];
        let assertions = graph.assertions_for(state_id);
        if assertions.is_empty() {
            continue;
        }

        // Copy state to temp dir to run assertions
        let (temp_dir, work_dir) = match copy_state_to_temp(state_id, graph) {
            Ok(pair) => pair,
            Err(e) => {
                steps.push(StepResult {
                    transition_name: format!("assertions on {}", state.name),
                    source_name: state.name.clone(),
                    target_name: state.name.clone(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e,
                    comparison: None,
                    output_diffs: Vec::new(),
                    assertion_results: Vec::new(),
                    passed: false,
                });
                passed = false;
                break;
            }
        };

        let assertion_results =
            run_assertions(&assertions, &work_dir, &state.env, graph, &opts.sandbox);
        let assertions_passed = assertion_results.iter().all(|a| a.passed);
        if !assertions_passed {
            passed = false;
        }

        // Determine a label — use transition name if available, else state name
        let label = if i > 0 {
            let ti = path.steps[i - 1];
            graph.transitions[ti].name.clone()
        } else {
            format!("(root) {}", state.name)
        };

        steps.push(StepResult {
            transition_name: label,
            source_name: state.name.clone(),
            target_name: state.name.clone(),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            comparison: None,
            output_diffs: Vec::new(),
            assertion_results,
            passed: assertions_passed,
        });

        if !opts.keep_temp {
            drop(temp_dir);
        }

        if !assertions_passed {
            break;
        }
    }

    PathResult {
        path_display,
        steps,
        passed,
    }
}

/// Full and NoCheck modes: execute transitions, compare filesystem, optionally run assertions.
fn run_path_transitions(
    graph: &StateGraph,
    path: &TestPath,
    path_display: String,
    opts: &RunOptions,
) -> PathResult {
    let mut steps = Vec::new();
    let mut passed = true;
    let run_assertions_flag = opts.check_mode == CheckMode::Full;

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
                        output_diffs: Vec::new(),
                        assertion_results: Vec::new(),
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
                            output_diffs: Vec::new(),
                            assertion_results: Vec::new(),
                            passed: false,
                        });
                        passed = false;
                        break;
                    }
                },
            }
        };

        // In Full mode, run source state assertions on the first step
        let mut source_assertion_results = Vec::new();
        if run_assertions_flag && step_idx == 0 {
            let source_assertions = graph.assertions_for(source.id);
            if !source_assertions.is_empty() {
                source_assertion_results = run_assertions(
                    &source_assertions,
                    &work_dir,
                    &source.env,
                    graph,
                    &opts.sandbox,
                );
            }
        }

        // Execute the transition command in the sandboxed env
        let step_result = execute_transition(
            transition,
            &work_dir,
            &source.env,
            target,
            graph,
            &opts.sandbox,
            run_assertions_flag,
        );

        // Merge source assertions into the step result (first step only)
        let mut step_result = step_result;
        if !source_assertion_results.is_empty() {
            let source_failed = source_assertion_results.iter().any(|a| !a.passed);
            step_result
                .assertion_results
                .splice(0..0, source_assertion_results);
            if source_failed {
                step_result.passed = false;
            }
        }

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

/// Run assertion commands against a state in a working directory.
fn run_assertions(
    assertions: &[&Assertion],
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    graph: &StateGraph,
    sandbox: &Sandbox,
) -> Vec<AssertionResult> {
    assertions
        .iter()
        .map(|assertion| run_single_assertion(assertion, work_dir, state_env, graph, sandbox))
        .collect()
}

/// Run a single assertion command and compare output.
fn run_single_assertion(
    assertion: &Assertion,
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    graph: &StateGraph,
    sandbox: &Sandbox,
) -> AssertionResult {
    let state = &graph.states[assertion.state.0];
    let bin_dir = state.path.join(&graph.config_dir).join("bin");
    let base_path = state_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or("/usr/local/bin:/usr/bin:/bin");
    let path_env = if bin_dir.exists() {
        format!("{}:{}", bin_dir, base_path)
    } else {
        base_path.to_string()
    };

    let output = match sandbox {
        Sandbox::None => build_assertion_command_bare(assertion, work_dir, state_env, &path_env),
        Sandbox::Flox {
            flox_bin,
            project_root,
        } => build_assertion_command_flox(
            assertion,
            work_dir,
            state_env,
            &path_env,
            flox_bin,
            project_root,
        ),
    };

    let output = match output {
        Some(result) => result,
        None => {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code: None,
                stdout_diff: None,
                stderr_diff: None,
                error: Some("empty command".into()),
            };
        }
    };

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code: None,
                stdout_diff: None,
                stderr_diff: None,
                error: Some(format!("failed to execute command: {e}")),
            };
        }
    };

    let exit_code = output.status.code();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    // Non-zero exit = failure
    if !output.status.success() {
        return AssertionResult {
            name: assertion.name.clone(),
            passed: false,
            exit_code,
            stdout_diff: None,
            stderr_diff: None,
            error: Some(format!(
                "command exited with {}",
                exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".into())
            )),
        };
    }

    // Compare stdout/stderr if expected values are specified
    let stdout_diff = assertion.expected_stdout.as_ref().and_then(|expected| {
        if *expected != stdout {
            Some((expected.clone(), stdout.clone()))
        } else {
            None
        }
    });

    let stderr_diff = assertion.expected_stderr.as_ref().and_then(|expected| {
        if *expected != stderr {
            Some((expected.clone(), stderr.clone()))
        } else {
            None
        }
    });

    let passed = stdout_diff.is_none() && stderr_diff.is_none();

    AssertionResult {
        name: assertion.name.clone(),
        passed,
        exit_code,
        stdout_diff,
        stderr_diff,
        error: None,
    }
}

/// Build a bare assertion command (no sandbox).
fn build_assertion_command_bare(
    assertion: &Assertion,
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
) -> Option<std::io::Result<std::process::Output>> {
    if assertion.shell {
        Some(
            Command::new("sh")
                .arg("-c")
                .arg(&assertion.command)
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(state_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    } else {
        let parts: Vec<&str> = assertion.command.split_whitespace().collect();
        if parts.is_empty() {
            return None;
        }
        Some(
            Command::new(parts[0])
                .args(&parts[1..])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(state_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    }
}

/// Build an assertion command wrapped in flox activate.
fn build_assertion_command_flox(
    assertion: &Assertion,
    work_dir: &Utf8Path,
    state_env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
    flox_bin: &Utf8Path,
    project_root: &Utf8Path,
) -> Option<std::io::Result<std::process::Output>> {
    if assertion.shell {
        Some(
            Command::new(flox_bin.as_str())
                .args([
                    "activate",
                    "-d",
                    project_root.as_str(),
                    "--",
                    "sh",
                    "-c",
                    &assertion.command,
                ])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(state_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    } else {
        let parts: Vec<&str> = assertion.command.split_whitespace().collect();
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
                .envs(state_env.iter())
                .env("PATH", path_env)
                .output(),
        )
    }
}

/// Execute a single transition command and compare the result.
fn execute_transition(
    transition: &Transition,
    work_dir: &Utf8Path,
    source_env: &std::collections::BTreeMap<String, String>,
    target: &crate::graph::State,
    graph: &StateGraph,
    sandbox: &Sandbox,
    run_assertions_flag: bool,
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
                output_diffs: Vec::new(),
                assertion_results: Vec::new(),
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
                output_diffs: Vec::new(),
                assertion_results: Vec::new(),
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
            output_diffs: Vec::new(),
            assertion_results: Vec::new(),
            passed: false,
        };
    }

    // Compare transition stdout/stderr if expected values are specified
    let mut output_diffs = Vec::new();
    if let Some(expected) = &transition.expected_stdout {
        if *expected != stdout {
            output_diffs.push(OutputDiff::StdoutMismatch {
                expected: expected.clone(),
                actual: stdout.clone(),
            });
        }
    }
    if let Some(expected) = &transition.expected_stderr {
        if *expected != stderr {
            output_diffs.push(OutputDiff::StderrMismatch {
                expected: expected.clone(),
                actual: stderr.clone(),
            });
        }
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
    let env_diffs = if !target.env.is_empty() || !transition.env_comparators.is_empty() {
        compare::compare_env(
            source_env,
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

    // Run target state assertions in Full mode
    let assertion_results = if run_assertions_flag {
        let target_assertions = graph.assertions_for(transition.target);
        if !target_assertions.is_empty() {
            run_assertions(&target_assertions, work_dir, &target.env, graph, sandbox)
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let assertions_passed = assertion_results.iter().all(|a| a.passed);
    let passed = comparison.passed && output_diffs.is_empty() && assertions_passed;

    StepResult {
        transition_name: transition.name.clone(),
        source_name,
        target_name,
        exit_code,
        stdout,
        stderr,
        comparison: Some(comparison),
        output_diffs,
        assertion_results,
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
