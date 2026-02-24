use std::collections::HashMap;
use std::io::IsTerminal;
use std::os::unix::io::AsRawFd;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use termimad::MadSkin;

use crate::compare::{EnvDiff, FileDiff, OutputDiff};
use crate::executor::{AssertionResult, PathResult, ProgressEvent, SetupResult, StepResult};

/// Disable terminal echo on stdin so keystrokes don't corrupt spinner output.
/// Returns the original termios settings for later restoration.
fn disable_echo() -> Option<libc::termios> {
    unsafe {
        let fd = std::io::stdin().as_raw_fd();
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut termios) != 0 {
            return None;
        }
        let original = termios;
        termios.c_lflag &= !(libc::ECHO);
        libc::tcsetattr(fd, libc::TCSANOW, &termios);
        Some(original)
    }
}

/// Restore original terminal settings.
fn restore_termios(original: &libc::termios) {
    unsafe {
        let fd = std::io::stdin().as_raw_fd();
        libc::tcsetattr(fd, libc::TCSANOW, original);
    }
}

/// Reports test execution progress to the terminal via per-path spinners.
pub struct ProgressReporter {
    mp: Option<MultiProgress>,
    /// Active (in-flight) bars, keyed by path index.
    bars: Mutex<HashMap<usize, (ProgressBar, String)>>,
    /// Finished bars that stay visible until final cleanup.
    finished: Mutex<Vec<ProgressBar>>,
    style: ProgressStyle,
    pass_style: ProgressStyle,
    fail_style: ProgressStyle,
    original_termios: Option<libc::termios>,
}

impl ProgressReporter {
    pub fn new() -> Self {
        if !std::io::stderr().is_terminal() {
            return Self {
                mp: None,
                bars: Mutex::new(HashMap::new()),
                finished: Mutex::new(Vec::new()),
                style: ProgressStyle::default_spinner(),
                pass_style: ProgressStyle::default_spinner(),
                fail_style: ProgressStyle::default_spinner(),
                original_termios: None,
            };
        }
        let style = ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏");
        let pass_style = ProgressStyle::with_template("{msg:.green}").unwrap();
        let fail_style = ProgressStyle::with_template("{msg:.red}").unwrap();
        // Disable echo so keystrokes don't corrupt spinner display.
        // Hide cursor for cleaner visual output.
        let original_termios = disable_echo();
        let term = console::Term::stderr();
        term.hide_cursor().ok();
        Self {
            mp: Some(MultiProgress::new()),
            bars: Mutex::new(HashMap::new()),
            finished: Mutex::new(Vec::new()),
            style,
            pass_style,
            fail_style,
            original_termios,
        }
    }

    pub fn on_event(&self, event: ProgressEvent) {
        let Some(mp) = &self.mp else { return };
        match event {
            ProgressEvent::PathStarted {
                index,
                total,
                display,
            } => {
                let bar = mp.add(ProgressBar::new_spinner());
                bar.set_style(self.style.clone());
                let msg = format!("[{}/{}] {display}", index + 1, total);
                bar.set_message(msg.clone());
                bar.enable_steady_tick(std::time::Duration::from_millis(80));
                self.bars.lock().unwrap().insert(index, (bar, msg));
            }
            ProgressEvent::PathFinished { index, passed } => {
                if let Some((bar, msg)) = self.bars.lock().unwrap().remove(&index) {
                    let (icon, style) = if passed {
                        ("✓", &self.pass_style)
                    } else {
                        ("✗", &self.fail_style)
                    };
                    bar.set_style(style.clone());
                    bar.finish_with_message(format!("{icon} {msg}"));
                    self.finished.lock().unwrap().push(bar);
                }
            }
            ProgressEvent::Interrupted => {
                for (_, (bar, _)) in self.bars.lock().unwrap().drain() {
                    bar.finish_with_message("interrupted");
                }
            }
        }
    }

    pub fn finish(&self) {
        let mut bars = self.bars.lock().unwrap();
        for (_, (bar, _)) in bars.drain() {
            bar.finish_and_clear();
        }
        for bar in self.finished.lock().unwrap().drain(..) {
            bar.finish_and_clear();
        }
        if let Some(mp) = &self.mp {
            mp.clear().ok();
            let term = console::Term::stderr();
            term.show_cursor().ok();
        }
        if let Some(ref original) = self.original_termios {
            restore_termios(original);
        }
    }
}

impl Drop for ProgressReporter {
    fn drop(&mut self) {
        if self.mp.is_some() {
            let term = console::Term::stderr();
            term.show_cursor().ok();
        }
        if let Some(ref original) = self.original_termios {
            restore_termios(original);
        }
    }
}

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
    let mut md = String::new();
    for result in results {
        format_path_result(&mut md, result, verbose);
    }
    md.push('\n');
    format_summary(&mut md, results);
    render(&md);

    results.iter().all(|r| r.passed)
}

fn terminal_width() -> Option<usize> {
    if std::io::stdout().is_terminal() {
        let term = console::Term::stdout();
        Some(term.size().1 as usize)
    } else {
        None
    }
}

/// Truncate a path display like "A → B → C → D → E" to fit within `max_width`
/// by replacing middle segments with "...".
fn truncate_path_display(display: &str, max_width: usize) -> String {
    if display.len() <= max_width {
        return display.to_string();
    }

    let segments: Vec<&str> = display.split(" → ").collect();
    if segments.len() <= 2 {
        // Can't meaningfully truncate with only 1-2 segments
        return display.to_string();
    }

    // Keep first and last, replace middle with "…"
    let first = segments[0];
    let last = segments[segments.len() - 1];
    let truncated = format!("{first} → … → {last}");

    if truncated.len() <= max_width {
        return truncated;
    }

    // Even first + last is too long, just return the truncated version anyway
    truncated
}

fn format_path_result(md: &mut String, result: &PathResult, verbose: bool) {
    let status = if result.passed {
        "**PASS**"
    } else {
        "**FAIL**"
    };
    let duration_str = format_duration(result.duration);
    // "**PASS** " = 9 chars rendered, plus duration and a space
    let overhead = 7 + duration_str.len() + 2; // "PASS " + " " + duration
    let path_display = if let Some(width) = terminal_width() {
        let available = width.saturating_sub(overhead);
        truncate_path_display(&result.path_display, available)
    } else {
        result.path_display.clone()
    };
    md.push_str(&format!("{status} {path_display} {duration_str}\n"));

    if !result.passed || verbose {
        for step in &result.steps {
            format_step_result(md, step, verbose);
        }
    }
}

fn format_step_result(md: &mut String, step: &StepResult, verbose: bool) {
    let icon = if step.passed { "✓" } else { "✗" };
    md.push_str(&format!(
        "  {icon} {} → {} ({}) {}\n",
        step.source_name,
        step.target_name,
        step.transition_name,
        format_duration(step.duration)
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

fn format_summary(md: &mut String, results: &[PathResult]) {
    let total = results.len();
    let passed = results.iter().filter(|r| r.passed).count();
    let failed = total - passed;
    let total_steps: usize = results.iter().map(|r| r.steps.len()).sum();
    let total_assertions: usize = results
        .iter()
        .flat_map(|r| &r.steps)
        .map(|s| s.assertion_results.len())
        .sum();
    let elapsed: Duration = results.iter().map(|r| r.duration).sum();

    let mut summary = format!("**{passed} passed, {failed} failed, {total} total");
    let mut details = Vec::new();
    if total_steps > 0 {
        details.push(format!("{total_steps} steps"));
    }
    if total_assertions > 0 {
        details.push(format!("{total_assertions} assertions"));
    }
    if !details.is_empty() {
        summary.push_str(&format!(" ({})", details.join(", ")));
    }
    summary.push_str(&format!(" in {}**\n", format_duration(elapsed)));
    md.push_str(&summary);
}

fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs < 0.001 {
        "<1ms".to_string()
    } else if secs < 1.0 {
        format!("{:.0}ms", secs * 1000.0)
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
