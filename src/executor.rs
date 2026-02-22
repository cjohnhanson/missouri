use std::process::Command;

use camino::{Utf8Path, Utf8PathBuf};
use tempfile::TempDir;

use crate::compare::{self, ComparisonResult, OutputDiff};
use crate::error;
use crate::graph::{Assertion, SandboxConfig, StateGraph, StateId, Transition};
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

/// Detect and prepare sandbox from project-level config.
///
/// Reads `graph.sandbox_config` to determine the sandbox mode:
/// - `SandboxConfig::None` → `Sandbox::None`
/// - `SandboxConfig::Packages(pkgs)` → generate manifest.toml, init flox env
/// - `SandboxConfig::Manifest(path)` → use user's manifest, init flox env
///
/// The managed flox environment lives in `<config_dir>/.flox/` inside the project root.
pub fn detect_sandbox(graph: &StateGraph) -> error::Result<Sandbox> {
    match &graph.sandbox_config {
        SandboxConfig::None => Ok(Sandbox::None),
        SandboxConfig::Packages(packages) => {
            let flox_bin = which_flox().ok_or_else(|| error::Error::FloxNotFound {
                root: graph.root.clone(),
            })?;
            let flox_dir =
                ensure_managed_flox_env(&graph.root, &graph.config_dir, &flox_bin, None, packages)?;
            Ok(Sandbox::Flox {
                flox_bin,
                project_root: flox_dir,
            })
        }
        SandboxConfig::Manifest(manifest_path) => {
            let flox_bin = which_flox().ok_or_else(|| error::Error::FloxNotFound {
                root: graph.root.clone(),
            })?;
            let flox_dir = ensure_managed_flox_env(
                &graph.root,
                &graph.config_dir,
                &flox_bin,
                Some(manifest_path),
                &[],
            )?;
            Ok(Sandbox::Flox {
                flox_bin,
                project_root: flox_dir,
            })
        }
    }
}

/// Generate a minimal manifest.toml from a list of package names.
fn generate_manifest(packages: &[String]) -> String {
    let mut manifest = String::from("version = 1\n\n[install]\n");
    for pkg in packages {
        manifest.push_str(&format!("{pkg}.pkg-path = \"{pkg}\"\n"));
    }
    manifest
}

/// Ensure the managed flox environment exists at `<root>/<config_dir>/.flox/`.
///
/// If `manifest_path` is Some, copies that manifest into the env.
/// If `manifest_path` is None, generates a manifest from `packages`.
///
/// Returns the absolute path to the directory containing `.flox/` (the managed env root).
fn ensure_managed_flox_env(
    root: &Utf8Path,
    config_dir: &str,
    flox_bin: &Utf8Path,
    manifest_path: Option<&Utf8Path>,
    packages: &[String],
) -> error::Result<Utf8PathBuf> {
    let managed_root = root.join(config_dir);
    let flox_dir = managed_root.join(".flox");
    let env_dir = flox_dir.join("env");

    if !flox_dir.exists() {
        // Initialize a new flox environment
        let output = Command::new(flox_bin.as_str())
            .args(["init", "-d", managed_root.as_str()])
            .output()
            .map_err(|e| error::Error::Io(e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(error::Error::FloxInitFailed {
                detail: stderr.into_owned(),
            });
        }
    }

    // Write the manifest
    let manifest_dest = env_dir.join("manifest.toml");
    let manifest_content = if let Some(path) = manifest_path {
        std::fs::read_to_string(path.as_std_path()).map_err(|e| error::Error::ConfigRead {
            path: path.to_owned(),
            source: e,
        })?
    } else {
        generate_manifest(packages)
    };
    std::fs::write(&manifest_dest, &manifest_content).map_err(|e| error::Error::Io(e))?;

    Ok(managed_root)
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

/// Build the PATH env var: state bin/ → project bin/ → base path.
fn build_path_env(
    state_bin: Option<&Utf8Path>,
    project_bin: Option<&Utf8Path>,
    base_path: &str,
) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(sb) = state_bin {
        parts.push(sb.as_str());
    }
    if let Some(pb) = project_bin {
        parts.push(pb.as_str());
    }
    parts.push(base_path);
    parts.join(":")
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

/// Configuration for recording transition output.
#[derive(Debug, Clone)]
pub struct RecordingConfig {
    /// Base output directory for recordings (e.g. `<root>/<config_dir>/runs/<run_id>/`).
    pub output_dir: Utf8PathBuf,
    /// The run ID.
    pub run_id: String,
}

/// Options for test execution.
pub struct RunOptions {
    pub keep_temp: bool,
    pub verbose: bool,
    pub sandbox: Sandbox,
    pub check_mode: CheckMode,
    /// If set, record transition output to .cast files.
    pub recording: Option<RecordingConfig>,
}

/// Result of running a single setup command.
#[derive(Debug)]
pub struct SetupResult {
    pub name: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run setup commands before test paths. Returns results and whether all passed.
pub fn run_setup_phase(graph: &StateGraph, opts: &RunOptions) -> Vec<SetupResult> {
    let base_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".into());
    let path_env = build_path_env(None, graph.project_bin.as_deref(), &base_path);

    graph
        .setup
        .iter()
        .scan(true, |still_passing, cmd| {
            if !*still_passing {
                return None; // stop after first failure
            }
            let result = run_single_setup(
                cmd,
                &graph.root,
                &path_env,
                &graph.project_env,
                &opts.sandbox,
            );
            if !result.passed {
                *still_passing = false;
            }
            Some(result)
        })
        .collect()
}

/// Run a single setup command.
fn run_single_setup(
    cmd: &crate::graph::SetupCommand,
    work_dir: &Utf8Path,
    path_env: &str,
    project_env: &std::collections::BTreeMap<String, String>,
    sandbox: &Sandbox,
) -> SetupResult {
    let output = if cmd.shell {
        match sandbox {
            Sandbox::None => Command::new("sh")
                .arg("-c")
                .arg(&cmd.command)
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(project_env.iter())
                .env("PATH", path_env)
                .output(),
            Sandbox::Flox {
                flox_bin,
                project_root,
            } => Command::new(flox_bin.as_str())
                .args([
                    "activate",
                    "-d",
                    project_root.as_str(),
                    "--",
                    "sh",
                    "-c",
                    &cmd.command,
                ])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(project_env.iter())
                .env("PATH", path_env)
                .env("SHELL", "/bin/sh")
                .output(),
        }
    } else {
        let parts: Vec<&str> = cmd.command.split_whitespace().collect();
        if parts.is_empty() {
            return SetupResult {
                name: cmd.name.clone(),
                passed: false,
                exit_code: None,
                stdout: String::new(),
                stderr: "empty command".into(),
            };
        }
        match sandbox {
            Sandbox::None => Command::new(parts[0])
                .args(&parts[1..])
                .current_dir(work_dir.as_std_path())
                .env_clear()
                .envs(project_env.iter())
                .env("PATH", path_env)
                .output(),
            Sandbox::Flox {
                flox_bin,
                project_root,
            } => {
                let mut args = vec!["activate", "-d", project_root.as_str(), "--"];
                args.extend(parts);
                Command::new(flox_bin.as_str())
                    .args(&args)
                    .current_dir(work_dir.as_std_path())
                    .env_clear()
                    .envs(project_env.iter())
                    .env("PATH", path_env)
                    .env("SHELL", "/bin/sh")
                    .output()
            }
        }
    };

    match output {
        Ok(o) => {
            let exit_code = o.status.code();
            SetupResult {
                name: cmd.name.clone(),
                passed: o.status.success(),
                exit_code,
                stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
            }
        }
        Err(e) => SetupResult {
            name: cmd.name.clone(),
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: format!("failed to execute command: {e}"),
        },
    }
}

/// Execute all test paths and return results.
pub fn run_all_paths(graph: &StateGraph, paths: &[TestPath], opts: &RunOptions) -> Vec<PathResult> {
    paths
        .iter()
        .enumerate()
        .map(|(path_idx, path)| run_path(graph, path, opts, path_idx))
        .collect()
}

/// Execute a single test path.
fn run_path(graph: &StateGraph, path: &TestPath, opts: &RunOptions, path_idx: usize) -> PathResult {
    let path_display = path.display(graph);

    match opts.check_mode {
        CheckMode::CheckOnly => run_path_check_only(graph, path, path_display, opts),
        CheckMode::Full | CheckMode::NoCheck => {
            run_path_transitions(graph, path, path_display, opts, path_idx)
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
    path_idx: usize,
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

        // Build recording path if recording is enabled
        let recording_path = opts.recording.as_ref().map(|rc| {
            let path_dir = rc.output_dir.join(format!("path-{path_idx}"));
            path_dir.join(format!("step-{step_idx}.cast"))
        });

        // Execute the transition command in the sandboxed env
        let step_result = execute_transition(
            transition,
            &work_dir,
            &source.env,
            target,
            graph,
            &opts.sandbox,
            run_assertions_flag,
            recording_path.as_ref(),
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
    let bin_dir_opt = if bin_dir.exists() {
        Some(bin_dir.as_path())
    } else {
        None
    };
    let base_path = state_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or("/usr/local/bin:/usr/bin:/bin");
    let path_env = build_path_env(bin_dir_opt, graph.project_bin.as_deref(), base_path);

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

    // Exit code check: should_fail inverts the expectation
    if assertion.should_fail {
        if output.status.success() {
            return AssertionResult {
                name: assertion.name.clone(),
                passed: false,
                exit_code,
                stdout_diff: None,
                stderr_diff: None,
                error: Some("expected command to fail, but it exited 0".into()),
            };
        }
        // Command failed as expected — pass (no stdout/stderr comparison for should_fail)
        return AssertionResult {
            name: assertion.name.clone(),
            passed: true,
            exit_code,
            stdout_diff: None,
            stderr_diff: None,
            error: None,
        };
    }

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
                .env("SHELL", "/bin/sh")
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
                .env("SHELL", "/bin/sh")
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
    recording_path: Option<&Utf8PathBuf>,
) -> StepResult {
    let source_name = graph.states[transition.source.0].name.clone();
    let target_name = target.name.clone();

    // Build PATH: source state's config bin/ → project bin/ → base PATH
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let bin_dir_opt = if bin_dir.exists() {
        Some(bin_dir.as_path())
    } else {
        None
    };
    let base_path = source_env
        .get("PATH")
        .map(|s| s.as_str())
        .unwrap_or("/usr/local/bin:/usr/bin:/bin");
    let path_env = build_path_env(bin_dir_opt, graph.project_bin.as_deref(), base_path);

    // Run the command — using recorder if recording is enabled, otherwise normal execution.
    let output = if let Some(cast_path) = recording_path {
        Some(crate::recorder::record_command(
            &transition.command,
            transition.shell,
            work_dir,
            source_env,
            &path_env,
            cast_path,
            sandbox,
        ))
    } else {
        match sandbox {
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
        }
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

    // Build bin dirs for comparator PATH: state bin/ + project bin/
    let source_state = &graph.states[transition.source.0];
    let bin_dir = source_state.path.join(&graph.config_dir).join("bin");
    let mut comparator_bin_dirs: Vec<&Utf8Path> = Vec::new();
    if bin_dir.exists() {
        comparator_bin_dirs.push(bin_dir.as_path());
    }
    if let Some(ref pb) = graph.project_bin {
        comparator_bin_dirs.push(pb.as_path());
    }

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
        &comparator_bin_dirs,
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
            &comparator_bin_dirs,
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
                .env("SHELL", "/bin/sh")
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
                .env("SHELL", "/bin/sh")
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
    fn detect_sandbox_none_when_no_config() {
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
        assert!(matches!(graph.sandbox_config, SandboxConfig::None));
        let sandbox = detect_sandbox(&graph).unwrap();
        assert!(matches!(sandbox, Sandbox::None));
    }

    #[test]
    fn detect_sandbox_packages_creates_env() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Create project-level config with packages
        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "packages:\n  - python3\n  - uv\n",
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
        assert!(matches!(graph.sandbox_config, SandboxConfig::Packages(_)));

        let sandbox = detect_sandbox(&graph).unwrap();
        match sandbox {
            Sandbox::Flox {
                flox_bin,
                project_root,
            } => {
                assert!(flox_bin.as_str().contains("flox"));
                // project_root should be the managed env inside .missouri/
                assert!(project_root.as_str().ends_with(".missouri"));
                // A .flox/ dir should have been created inside .missouri/
                assert!(project_root.join(".flox").exists());
                // manifest.toml should contain our packages
                let manifest =
                    fs::read_to_string(project_root.join(".flox/env/manifest.toml")).unwrap();
                assert!(manifest.contains("python3"));
                assert!(manifest.contains("uv"));
            }
            Sandbox::None => panic!("expected Sandbox::Flox"),
        }
    }

    #[test]
    fn detect_sandbox_manifest_creates_env() {
        let tmp = tempfile::tempdir().unwrap();
        let root = Utf8Path::from_path(tmp.path()).unwrap();

        // Write a user manifest.toml at the project root
        let manifest_content = "version = 1\n\n[install]\ncargo.pkg-path = \"cargo\"\n";
        fs::write(root.join("manifest.toml"), manifest_content).unwrap();

        // Create project-level config pointing to the manifest
        let root_missouri = root.join(".missouri");
        fs::create_dir_all(&root_missouri).unwrap();
        fs::write(
            root_missouri.join("missouri.yml"),
            "flox:\n  manifest: \"manifest.toml\"\n",
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
        assert!(matches!(graph.sandbox_config, SandboxConfig::Manifest(_)));

        let sandbox = detect_sandbox(&graph).unwrap();
        match sandbox {
            Sandbox::Flox {
                flox_bin,
                project_root,
            } => {
                assert!(flox_bin.as_str().contains("flox"));
                assert!(project_root.join(".flox").exists());
                // The user's manifest should have been copied in
                let manifest =
                    fs::read_to_string(project_root.join(".flox/env/manifest.toml")).unwrap();
                assert!(manifest.contains("cargo"));
            }
            Sandbox::None => panic!("expected Sandbox::Flox"),
        }
    }

    #[test]
    fn generate_manifest_from_packages() {
        let manifest = generate_manifest(&["python3".into(), "uv".into()]);
        assert!(manifest.contains("version = 1"));
        assert!(manifest.contains("python3.pkg-path = \"python3\""));
        assert!(manifest.contains("uv.pkg-path = \"uv\""));
    }

    #[test]
    fn which_flox_finds_binary() {
        // flox is installed on this system
        let result = which_flox();
        assert!(result.is_some());
        assert!(result.unwrap().as_str().ends_with("flox"));
    }
}
