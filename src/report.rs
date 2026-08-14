use std::io::{self, Write};
use std::time::Duration;

use crate::compare::{EnvDiff, FileDiff, OutputDiff};
use crate::executor::{AssertionResult, PathResult, ProgressEvent, SetupResult, StepResult};

/// Reports test progress as plain lines on stderr. It does not move the
/// cursor, hide the cursor, or suppress echo.
pub struct ProgressReporter {
    total: usize,
}

impl ProgressReporter {
    pub fn new() -> Self {
        Self { total: 0 }
    }

    pub fn prepare(&mut self, paths: &[crate::paths::TestPath], _graph: &crate::graph::StateGraph) {
        self.total = paths.len();
    }

    pub fn on_event(&self, event: ProgressEvent) {
        match event {
            ProgressEvent::PathStarted { .. } => {}
            ProgressEvent::PathFinished { index, passed } => {
                let icon = if passed {
                    "\x1b[32m✓\x1b[0m"
                } else {
                    "\x1b[31m✗\x1b[0m"
                };
                eprintln!("{icon} [{}/{}]", index + 1, self.total);
            }
            ProgressEvent::Interrupted => {
                eprintln!("interrupted");
            }
        }
    }

    pub fn finish(&self) {}
}

impl Default for ProgressReporter {
    fn default() -> Self {
        Self::new()
    }
}

/// Print results for all test paths. Returns true if all passed.
pub fn print_results(results: &[PathResult], verbose: bool) -> bool {
    let out = &mut io::stdout().lock();

    for result in results {
        print_path_result(out, result, verbose);
    }

    writeln!(out).ok();
    print_summary(out, results);

    results.iter().all(|r| r.passed)
}

fn print_path_result(out: &mut impl Write, result: &PathResult, verbose: bool) {
    let icon = if result.passed {
        "\x1b[32mPASS\x1b[0m"
    } else {
        "\x1b[31mFAIL\x1b[0m"
    };
    writeln!(
        out,
        "{icon} {} {}",
        result.path_display,
        fmt_duration(result.duration)
    )
    .ok();

    if !result.passed || verbose {
        for step in &result.steps {
            print_step_result(out, step, verbose);
        }
    }
}

fn print_step_result(out: &mut impl Write, step: &StepResult, verbose: bool) {
    let icon = if step.passed {
        "\x1b[32m✓\x1b[0m"
    } else {
        "\x1b[31m✗\x1b[0m"
    };
    writeln!(
        out,
        "  {icon} {} → {} ({}) {}",
        step.source_name,
        step.target_name,
        step.transition_name,
        fmt_duration(step.duration)
    )
    .ok();

    if !step.passed {
        // Command failure
        if step.comparison.is_none() && step.assertion_results.is_empty() {
            let exit_str = step
                .exit_code
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "no exit code".into());
            writeln!(out, "    command failed: {exit_str}").ok();
            for line in step.stderr.lines().take(10) {
                writeln!(out, "    {line}").ok();
            }
        }

        // Output diffs
        for diff in &step.output_diffs {
            print_output_diff(out, diff);
        }

        // Comparison diffs
        if let Some(comparison) = &step.comparison {
            for diff in &comparison.file_diffs {
                print_file_diff(out, diff);
            }
            for diff in &comparison.env_diffs {
                print_env_diff(out, diff);
            }
        }

        // Failed assertions
        for result in &step.assertion_results {
            if !result.passed {
                print_assertion_result(out, result);
            }
        }
    } else if verbose {
        // Passing assertions in verbose mode
        for result in &step.assertion_results {
            let icon = if result.passed {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[31m✗\x1b[0m"
            };
            writeln!(
                out,
                "    {icon} assert: {} {}",
                result.name,
                fmt_duration(result.duration)
            )
            .ok();
        }
    }
}

fn print_output_diff(out: &mut impl Write, diff: &OutputDiff) {
    match diff {
        OutputDiff::StdoutMismatch { expected, actual } => {
            writeln!(out, "    stdout mismatch:").ok();
            writeln!(out, "      expected: {expected:?}").ok();
            writeln!(out, "      actual:   {actual:?}").ok();
        }
        OutputDiff::StderrMismatch { expected, actual } => {
            writeln!(out, "    stderr mismatch:").ok();
            writeln!(out, "      expected: {expected:?}").ok();
            writeln!(out, "      actual:   {actual:?}").ok();
        }
    }
}

fn print_assertion_result(out: &mut impl Write, result: &AssertionResult) {
    writeln!(
        out,
        "    \x1b[31m✗\x1b[0m assert: {} {}",
        result.name,
        fmt_duration(result.duration)
    )
    .ok();
    if let Some(error) = &result.error {
        writeln!(out, "      {error}").ok();
    }
    if let Some((expected, actual)) = &result.stdout_diff {
        writeln!(out, "      stdout expected: {expected:?}").ok();
        writeln!(out, "      stdout actual:   {actual:?}").ok();
    }
    if let Some((expected, actual)) = &result.stderr_diff {
        writeln!(out, "      stderr expected: {expected:?}").ok();
        writeln!(out, "      stderr actual:   {actual:?}").ok();
    }
}

fn print_file_diff(out: &mut impl Write, diff: &FileDiff) {
    match diff {
        FileDiff::ExtraFile { path } => {
            writeln!(out, "    extra: {path}").ok();
        }
        FileDiff::MissingFile { path } => {
            writeln!(out, "    missing: {path}").ok();
        }
        FileDiff::ContentMismatch { path, detail } => {
            writeln!(out, "    differs: {path}").ok();
            for line in detail.lines().take(10) {
                writeln!(out, "      {line}").ok();
            }
        }
        FileDiff::ComparatorFailed {
            path,
            command,
            stderr,
        } => {
            writeln!(out, "    comparator failed: {path} ({command})").ok();
            for line in stderr.lines().take(5) {
                writeln!(out, "      {line}").ok();
            }
        }
    }
}

fn print_env_diff(out: &mut impl Write, diff: &EnvDiff) {
    match diff {
        EnvDiff::ExtraVar { name } => {
            writeln!(out, "    extra env: {name}").ok();
        }
        EnvDiff::MissingVar { name } => {
            writeln!(out, "    missing env: {name}").ok();
        }
        EnvDiff::ValueMismatch {
            name,
            actual,
            expected,
        } => {
            writeln!(
                out,
                "    env differs: {name} (expected: {expected:?}, actual: {actual:?})"
            )
            .ok();
        }
        EnvDiff::ComparatorFailed {
            name,
            command,
            stderr,
        } => {
            writeln!(out, "    env comparator failed: {name} ({command})").ok();
            for line in stderr.lines().take(5) {
                writeln!(out, "      {line}").ok();
            }
        }
    }
}

fn print_summary(out: &mut impl Write, results: &[PathResult]) {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;
    let total_steps: usize = results.iter().map(|r| r.steps.len()).sum();
    let total_assertions: usize = results
        .iter()
        .flat_map(|r| &r.steps)
        .map(|s| s.assertion_results.len())
        .sum();
    let wall_time: Duration = results.iter().map(|r| r.duration).max().unwrap_or_default();
    let cpu_time: Duration = results.iter().map(|r| r.duration).sum();

    // Summary line
    let mut parts = vec![format!("{passed} passed"), format!("{failed} failed")];
    if total_steps > 0 {
        parts.push(format!("{total_steps} steps"));
    }
    if total_assertions > 0 {
        parts.push(format!("{total_assertions} assertions"));
    }
    writeln!(out, "{} in {}", parts.join(", "), fmt_duration(wall_time)).ok();

    // Show CPU time if significantly different from wall time (parallel execution)
    if cpu_time > wall_time + Duration::from_secs(5) {
        writeln!(out, "  (total CPU time: {})", fmt_duration(cpu_time)).ok();
    }

    // Slowest transitions
    let mut transitions: Vec<(&str, &str, &str, Duration)> = results
        .iter()
        .flat_map(|r| &r.steps)
        .map(|s| {
            (
                s.source_name.as_str(),
                s.target_name.as_str(),
                s.transition_name.as_str(),
                s.duration,
            )
        })
        .collect();
    transitions.sort_by(|a, b| b.3.cmp(&a.3));

    if transitions.len() > 1 {
        writeln!(out).ok();
        writeln!(out, "Slowest transitions:").ok();
        for (src, tgt, name, dur) in transitions.iter().take(5) {
            writeln!(out, "  {} → {} ({}) {}", src, tgt, name, fmt_duration(*dur)).ok();
        }
    }

    // Slowest assertions (only if any are notably slow)
    let mut assertions: Vec<(&str, Duration)> = results
        .iter()
        .flat_map(|r| &r.steps)
        .flat_map(|s| &s.assertion_results)
        .map(|a| (a.name.as_str(), a.duration))
        .collect();
    assertions.sort_by(|a, b| b.1.cmp(&a.1));

    if let Some((_, top_dur)) = assertions.first() {
        if *top_dur > Duration::from_secs(1) {
            writeln!(out).ok();
            writeln!(out, "Slowest assertions:").ok();
            for (name, dur) in assertions.iter().take(5) {
                if *dur > Duration::from_millis(500) {
                    writeln!(out, "  {name} {}", fmt_duration(*dur)).ok();
                }
            }
        }
    }
}

fn fmt_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 0.001 {
        "<1ms".to_string()
    } else if secs < 1.0 {
        format!("{}ms", (secs * 1000.0) as u64)
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let mins = secs as u64 / 60;
        let remaining = secs - (mins as f64 * 60.0);
        format!("{mins}m{remaining:.1}s")
    }
}

/// Print the list of states.
pub fn print_states(graph: &crate::graph::StateGraph) {
    for state in &graph.states {
        println!("{}", state.name);
    }
}

/// Print the list of transitions.
pub fn print_transitions(graph: &crate::graph::StateGraph) {
    for t in &graph.transitions {
        let source = &graph.states[t.source.0].name;
        let target = &graph.states[t.target.0].name;
        println!("{source} → {target} ({})", t.name);
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

/// Print setup command results. Returns true if all passed.
pub fn print_setup_results(results: &[SetupResult], verbose: bool) -> bool {
    for result in results {
        let icon = if result.passed {
            "\x1b[32m✓\x1b[0m"
        } else {
            "\x1b[31m✗\x1b[0m"
        };
        println!("{icon} setup: {}", result.name);

        if !result.passed {
            let exit_str = result
                .exit_code
                .map(|c| format!("exit code {c}"))
                .unwrap_or_else(|| "no exit code".into());
            println!("  {exit_str}");
            for line in result.stderr.lines().take(10) {
                println!("  {line}");
            }
        } else if verbose && !result.stdout.is_empty() {
            for line in result.stdout.lines().take(5) {
                println!("  {line}");
            }
        }
    }

    results.iter().all(|r| r.passed)
}
