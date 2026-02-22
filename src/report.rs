use owo_colors::OwoColorize;
use supports_color::Stream;

use crate::compare::{EnvDiff, FileDiff, OutputDiff};
use crate::executor::{AssertionResult, PathResult, StepResult};

fn use_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

/// Print results for all test paths. Returns true if all passed.
pub fn print_results(results: &[PathResult], verbose: bool) -> bool {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    for result in results {
        print_path_result(result, verbose);
    }

    println!();
    print_summary(total, passed, failed);

    failed == 0
}

fn print_path_result(result: &PathResult, verbose: bool) {
    let status = if result.passed {
        if use_color() {
            "PASS".green().bold().to_string()
        } else {
            "PASS".into()
        }
    } else if use_color() {
        "FAIL".red().bold().to_string()
    } else {
        "FAIL".into()
    };

    println!("{status} {}", result.path_display);

    if !result.passed || verbose {
        for step in &result.steps {
            print_step_result(step, verbose);
        }
    }
}

fn print_step_result(step: &StepResult, verbose: bool) {
    let arrow = if use_color() {
        "→".dimmed().to_string()
    } else {
        "→".into()
    };

    let status_indicator = if step.passed {
        if use_color() {
            "✓".green().to_string()
        } else {
            "✓".into()
        }
    } else if use_color() {
        "✗".red().to_string()
    } else {
        "✗".into()
    };

    println!(
        "    {status_indicator} {} {arrow} {} ({})",
        step.source_name, step.target_name, step.transition_name
    );

    if !step.passed {
        // Show command failure info
        if step.comparison.is_none() && step.assertion_results.is_empty() {
            let exit_str = step
                .exit_code
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "no exit code".into());
            println!("      command failed: {exit_str}");
            if !step.stderr.is_empty() {
                for line in step.stderr.lines().take(10) {
                    println!("      {line}");
                }
            }
        }

        // Show output diffs
        for diff in &step.output_diffs {
            print_output_diff(diff);
        }

        // Show comparison diffs
        if let Some(comparison) = &step.comparison {
            for diff in &comparison.file_diffs {
                print_file_diff(diff);
            }
            for diff in &comparison.env_diffs {
                print_env_diff(diff);
            }
        }

        // Show assertion results
        for result in &step.assertion_results {
            if !result.passed {
                print_assertion_result(result);
            }
        }
    } else if verbose {
        if !step.stdout.is_empty() {
            for line in step.stdout.lines().take(5) {
                println!("      {line}");
            }
        }
        // In verbose mode, show passing assertions too
        for result in &step.assertion_results {
            let status = if use_color() {
                "✓ assert".green().to_string()
            } else {
                "✓ assert".into()
            };
            println!("      {status}: {}", result.name);
        }
    }
}

fn print_output_diff(diff: &OutputDiff) {
    match diff {
        OutputDiff::StdoutMismatch { expected, actual } => {
            let label = if use_color() {
                "stdout mismatch".red().to_string()
            } else {
                "stdout mismatch".into()
            };
            println!("      {label}:");
            println!("        expected: {expected:?}");
            println!("        actual:   {actual:?}");
        }
        OutputDiff::StderrMismatch { expected, actual } => {
            let label = if use_color() {
                "stderr mismatch".red().to_string()
            } else {
                "stderr mismatch".into()
            };
            println!("      {label}:");
            println!("        expected: {expected:?}");
            println!("        actual:   {actual:?}");
        }
    }
}

fn print_assertion_result(result: &AssertionResult) {
    let label = if use_color() {
        "✗ assert".red().to_string()
    } else {
        "✗ assert".into()
    };
    println!("      {label}: {}", result.name);
    if let Some(error) = &result.error {
        println!("        {error}");
    }
    if let Some((expected, actual)) = &result.stdout_diff {
        println!("        stdout expected: {expected:?}");
        println!("        stdout actual:   {actual:?}");
    }
    if let Some((expected, actual)) = &result.stderr_diff {
        println!("        stderr expected: {expected:?}");
        println!("        stderr actual:   {actual:?}");
    }
}

fn print_file_diff(diff: &FileDiff) {
    match diff {
        FileDiff::ExtraFile { path } => {
            let label = if use_color() {
                "extra".yellow().to_string()
            } else {
                "extra".into()
            };
            println!("      {label}: {path}");
        }
        FileDiff::MissingFile { path } => {
            let label = if use_color() {
                "missing".red().to_string()
            } else {
                "missing".into()
            };
            println!("      {label}: {path}");
        }
        FileDiff::ContentMismatch { path, detail } => {
            let label = if use_color() {
                "differs".red().to_string()
            } else {
                "differs".into()
            };
            println!("      {label}: {path}");
            for line in detail.lines().take(10) {
                println!("        {line}");
            }
        }
        FileDiff::ComparatorFailed {
            path,
            command,
            stderr,
        } => {
            let label = if use_color() {
                "comparator failed".red().to_string()
            } else {
                "comparator failed".into()
            };
            println!("      {label}: {path} ({command})");
            for line in stderr.lines().take(5) {
                println!("        {line}");
            }
        }
    }
}

fn print_env_diff(diff: &EnvDiff) {
    match diff {
        EnvDiff::ExtraVar { name } => {
            let label = if use_color() {
                "extra env".yellow().to_string()
            } else {
                "extra env".into()
            };
            println!("      {label}: {name}");
        }
        EnvDiff::MissingVar { name } => {
            let label = if use_color() {
                "missing env".red().to_string()
            } else {
                "missing env".into()
            };
            println!("      {label}: {name}");
        }
        EnvDiff::ValueMismatch {
            name,
            actual,
            expected,
        } => {
            let label = if use_color() {
                "env differs".red().to_string()
            } else {
                "env differs".into()
            };
            println!("      {label}: {name} (expected: {expected:?}, actual: {actual:?})");
        }
        EnvDiff::ComparatorFailed {
            name,
            command,
            stderr,
        } => {
            let label = if use_color() {
                "env comparator failed".red().to_string()
            } else {
                "env comparator failed".into()
            };
            println!("      {label}: {name} ({command})");
            for line in stderr.lines().take(5) {
                println!("        {line}");
            }
        }
    }
}

fn print_summary(total: usize, passed: usize, failed: usize) {
    let summary = format!("{passed} passed, {failed} failed, {total} total");
    if failed == 0 {
        if use_color() {
            println!("{}", summary.green().bold());
        } else {
            println!("{summary}");
        }
    } else if use_color() {
        println!("{}", summary.red().bold());
    } else {
        println!("{summary}");
    }
}

/// Print the list of states.
pub fn print_states(graph: &crate::graph::StateGraph) {
    for state in &graph.states {
        println!("{}", state.name);
        if !state.env.is_empty() {
            for (k, v) in &state.env {
                let label = if use_color() {
                    "env".dimmed().to_string()
                } else {
                    "env".into()
                };
                println!("  {label}: {k}={v}");
            }
        }
    }
}

/// Print the list of transitions.
pub fn print_transitions(graph: &crate::graph::StateGraph) {
    for t in &graph.transitions {
        let source = &graph.states[t.source.0].name;
        let target = &graph.states[t.target.0].name;
        println!("{} → {} ({})", source, target, t.name);
    }
}

/// Print all enumerated test paths.
pub fn print_paths(paths: &[crate::paths::TestPath], graph: &crate::graph::StateGraph) {
    for (i, path) in paths.iter().enumerate() {
        println!("{}. {}", i + 1, path.display(graph));
    }
    println!();
    println!("{} path(s)", paths.len());
}
