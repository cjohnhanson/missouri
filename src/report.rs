use std::io::IsTerminal;

use termimad::MadSkin;

use crate::compare::{EnvDiff, FileDiff, OutputDiff};
use crate::executor::{AssertionResult, PathResult, SetupResult, StepResult};

fn skin() -> MadSkin {
    if std::io::stdout().is_terminal() {
        MadSkin::default()
    } else {
        MadSkin::no_style()
    }
}

fn render(md: &str) {
    let skin = skin();
    print!("{}", skin.term_text(md));
}

/// Print results for all test paths. Returns true if all passed.
pub fn print_results(results: &[PathResult], verbose: bool) -> bool {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;

    let mut md = String::new();
    for result in results {
        format_path_result(&mut md, result, verbose);
    }
    md.push('\n');
    format_summary(&mut md, total, passed, failed);
    render(&md);

    failed == 0
}

fn format_path_result(md: &mut String, result: &PathResult, verbose: bool) {
    let status = if result.passed {
        "**PASS**"
    } else {
        "**FAIL**"
    };
    md.push_str(&format!("{status} {}\n", result.path_display));

    if !result.passed || verbose {
        for step in &result.steps {
            format_step_result(md, step, verbose);
        }
    }
}

fn format_step_result(md: &mut String, step: &StepResult, verbose: bool) {
    let icon = if step.passed { "✓" } else { "✗" };
    md.push_str(&format!(
        "  {icon} {} → {} ({})\n",
        step.source_name, step.target_name, step.transition_name
    ));

    if !step.passed {
        // Command failure info
        if step.comparison.is_none() && step.assertion_results.is_empty() {
            let exit_str = step
                .exit_code
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "no exit code".into());
            md.push_str(&format!("    command failed: {exit_str}\n"));
            if !step.stderr.is_empty() {
                for line in step.stderr.lines().take(10) {
                    md.push_str(&format!("    {line}\n"));
                }
            }
        }

        // Output diffs
        for diff in &step.output_diffs {
            format_output_diff(md, diff);
        }

        // Comparison diffs
        if let Some(comparison) = &step.comparison {
            for diff in &comparison.file_diffs {
                format_file_diff(md, diff);
            }
            for diff in &comparison.env_diffs {
                format_env_diff(md, diff);
            }
        }

        // Failed assertions
        for result in &step.assertion_results {
            if !result.passed {
                format_assertion_result(md, result);
            }
        }
    } else if verbose {
        if !step.stdout.is_empty() {
            for line in step.stdout.lines().take(5) {
                md.push_str(&format!("    {line}\n"));
            }
        }
        // Passing assertions in verbose mode
        for result in &step.assertion_results {
            md.push_str(&format!("    ✓ assert: {}\n", result.name));
        }
    }
}

fn format_output_diff(md: &mut String, diff: &OutputDiff) {
    match diff {
        OutputDiff::StdoutMismatch { expected, actual } => {
            md.push_str("    **stdout mismatch**:\n");
            md.push_str(&format!("      expected: {expected:?}\n"));
            md.push_str(&format!("      actual:   {actual:?}\n"));
        }
        OutputDiff::StderrMismatch { expected, actual } => {
            md.push_str("    **stderr mismatch**:\n");
            md.push_str(&format!("      expected: {expected:?}\n"));
            md.push_str(&format!("      actual:   {actual:?}\n"));
        }
    }
}

fn format_assertion_result(md: &mut String, result: &AssertionResult) {
    md.push_str(&format!("    ✗ assert: {}\n", result.name));
    if let Some(error) = &result.error {
        md.push_str(&format!("      {error}\n"));
    }
    if let Some((expected, actual)) = &result.stdout_diff {
        md.push_str(&format!("      stdout expected: {expected:?}\n"));
        md.push_str(&format!("      stdout actual:   {actual:?}\n"));
    }
    if let Some((expected, actual)) = &result.stderr_diff {
        md.push_str(&format!("      stderr expected: {expected:?}\n"));
        md.push_str(&format!("      stderr actual:   {actual:?}\n"));
    }
}

fn format_file_diff(md: &mut String, diff: &FileDiff) {
    match diff {
        FileDiff::ExtraFile { path } => {
            md.push_str(&format!("    extra: {path}\n"));
        }
        FileDiff::MissingFile { path } => {
            md.push_str(&format!("    missing: {path}\n"));
        }
        FileDiff::ContentMismatch { path, detail } => {
            md.push_str(&format!("    differs: {path}\n"));
            for line in detail.lines().take(10) {
                md.push_str(&format!("      {line}\n"));
            }
        }
        FileDiff::ComparatorFailed {
            path,
            command,
            stderr,
        } => {
            md.push_str(&format!("    comparator failed: {path} ({command})\n"));
            for line in stderr.lines().take(5) {
                md.push_str(&format!("      {line}\n"));
            }
        }
    }
}

fn format_env_diff(md: &mut String, diff: &EnvDiff) {
    match diff {
        EnvDiff::ExtraVar { name } => {
            md.push_str(&format!("    extra env: {name}\n"));
        }
        EnvDiff::MissingVar { name } => {
            md.push_str(&format!("    missing env: {name}\n"));
        }
        EnvDiff::ValueMismatch {
            name,
            actual,
            expected,
        } => {
            md.push_str(&format!(
                "    env differs: {name} (expected: {expected:?}, actual: {actual:?})\n"
            ));
        }
        EnvDiff::ComparatorFailed {
            name,
            command,
            stderr,
        } => {
            md.push_str(&format!("    env comparator failed: {name} ({command})\n"));
            for line in stderr.lines().take(5) {
                md.push_str(&format!("      {line}\n"));
            }
        }
    }
}

fn format_summary(md: &mut String, total: usize, passed: usize, failed: usize) {
    if failed == 0 {
        md.push_str(&format!(
            "**{passed} passed, {failed} failed, {total} total**\n"
        ));
    } else {
        md.push_str(&format!(
            "**{passed} passed, {failed} failed, {total} total**\n"
        ));
    }
}

/// Print the list of states.
pub fn print_states(graph: &crate::graph::StateGraph) {
    let mut md = String::new();
    for state in &graph.states {
        md.push_str(&format!("**{}**\n", state.name));
        if !state.env.is_empty() {
            for (k, v) in &state.env {
                md.push_str(&format!("  env: {k}={v}\n"));
            }
        }
    }
    render(&md);
}

/// Print the list of transitions.
pub fn print_transitions(graph: &crate::graph::StateGraph) {
    let mut md = String::new();
    for t in &graph.transitions {
        let source = &graph.states[t.source.0].name;
        let target = &graph.states[t.target.0].name;
        md.push_str(&format!("{source} → {target} ({})\n", t.name));
    }
    render(&md);
}

/// Print all enumerated test paths.
pub fn print_paths(paths: &[crate::paths::TestPath], graph: &crate::graph::StateGraph) {
    let mut md = String::new();
    for (i, path) in paths.iter().enumerate() {
        md.push_str(&format!("{}. {}\n", i + 1, path.display(graph)));
    }
    md.push('\n');
    md.push_str(&format!("{} path(s)\n", paths.len()));
    render(&md);
}

/// Print setup command results. Returns true if all passed.
pub fn print_setup_results(results: &[SetupResult], verbose: bool) -> bool {
    let mut md = String::new();
    for result in results {
        let icon = if result.passed { "✓" } else { "✗" };
        md.push_str(&format!("{icon} setup: {}\n", result.name));

        if !result.passed {
            let exit_str = result
                .exit_code
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "no exit code".into());
            md.push_str(&format!("  {exit_str}\n"));
            if !result.stderr.is_empty() {
                for line in result.stderr.lines().take(10) {
                    md.push_str(&format!("  {line}\n"));
                }
            }
        } else if verbose && !result.stdout.is_empty() {
            for line in result.stdout.lines().take(5) {
                md.push_str(&format!("  {line}\n"));
            }
        }
    }
    render(&md);

    results.iter().all(|r| r.passed)
}
