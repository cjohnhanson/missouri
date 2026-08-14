//! Recording module. It captures the transition output as asciicast v2.
//!
//! Asciicast v2 format:
//!   Line 1: JSON header `{"version":2,"width":80,"height":24,...}`
//!   Lines 2+: `[timestamp, "o", data]`

use std::io::Write;
use std::process::Stdio;
use std::time::Instant;

use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;

use crate::error;

/// Fixed terminal dimensions for recording.
const TERM_WIDTH: u16 = 80;
const TERM_HEIGHT: u16 = 24;

/// Record a command execution, producing a .cast file.
///
/// Runs `command` through `sh -c` in `work_dir` with the given
/// environment. Returns the process Output, which holds the status, the
/// stdout, and the stderr. Writes a `.cast` file to `cast_path`.
pub fn record_command(
    command: &str,
    shell: bool,
    work_dir: &Utf8Path,
    env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
    cast_path: &Utf8Path,
    sandbox: &dyn crate::executor::Backend,
) -> std::io::Result<std::process::Output> {
    let mut child = build_recording_command(command, shell, work_dir, env, path_env, sandbox)?;
    let signal_slot = crate::signal::register_child(child.id());

    let start = Instant::now();
    let mut events: Vec<(f64, String)> = Vec::new();

    let stdout_handle = child.stdout.take().unwrap();
    let stderr_handle = child.stderr.take().unwrap();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::BufReader::new(stdout_handle), &mut buf).ok();
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        std::io::Read::read_to_end(&mut std::io::BufReader::new(stderr_handle), &mut buf).ok();
        buf
    });

    let stdout_bytes = stdout_thread.join().unwrap_or_default();
    let stderr_bytes = stderr_thread.join().unwrap_or_default();
    let status = child.wait()?;
    crate::signal::clear_child(signal_slot);
    let elapsed = start.elapsed().as_secs_f64();

    let stdout_str = String::from_utf8_lossy(&stdout_bytes);
    let stderr_str = String::from_utf8_lossy(&stderr_bytes);

    // Collect all output lines (stdout first, then stderr) for replay timing.
    let mut all_lines: Vec<String> = Vec::new();
    for line in stdout_str.lines() {
        all_lines.push(format!("{line}\r\n"));
    }
    for line in stderr_str.lines() {
        all_lines.push(format!("{line}\r\n"));
    }

    // Spread the lines across the recording. The recording lasts 3s at
    // least, or 150ms for each line.
    let min_by_lines = all_lines.len() as f64 * 0.15;
    let replay_duration = elapsed.max(min_by_lines).max(3.0);

    if all_lines.is_empty() {
        events.push((0.0, " \r\n".to_string()));
    } else {
        let time_per_line = (replay_duration * 0.8) / all_lines.len() as f64;
        for (i, line) in all_lines.into_iter().enumerate() {
            events.push((time_per_line * i as f64, line));
        }
    }

    write_cast_file(cast_path, &events, replay_duration)?;

    Ok(std::process::Output {
        status,
        stdout: stdout_bytes,
        stderr: stderr_bytes,
    })
}

fn build_recording_command(
    command: &str,
    shell: bool,
    work_dir: &Utf8Path,
    env: &std::collections::BTreeMap<String, String>,
    path_env: &str,
    sandbox: &dyn crate::executor::Backend,
) -> std::io::Result<std::process::Child> {
    let mut cmd = if shell {
        sandbox.build_shell_command(command, work_dir, env, path_env)
    } else {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "empty command",
            ));
        }
        sandbox.build_direct_command(&parts, work_dir, env, path_env)
    };
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
}

/// Write asciicast v2 format.
fn write_cast_file(
    path: &Utf8Path,
    events: &[(f64, String)],
    _total_duration: f64,
) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(path)?;

    let header = json!({
        "version": 2,
        "width": TERM_WIDTH,
        "height": TERM_HEIGHT,
        "timestamp": chrono::Utc::now().timestamp(),
    });
    writeln!(f, "{}", header)?;

    for (t, data) in events {
        let event = json!([t, "o", data]);
        writeln!(f, "{}", event)?;
    }

    Ok(())
}

/// Extract raw text output from a .cast file by concatenating all event data.
fn extract_cast_output(cast_path: &Utf8Path) -> Option<String> {
    let content = std::fs::read_to_string(cast_path).ok()?;
    let mut output = String::new();
    for line in content.lines().skip(1) {
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(line)
            && let Some(data) = event.get(2).and_then(|v| v.as_str())
        {
            let clean = strip_ansi(&data.replace('\r', ""));
            output.push_str(&clean);
        }
    }
    let trimmed = output.trim_end().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Strip ANSI escape sequences from a string.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if let Some('[') = chars.next() {
                for c2 in chars.by_ref() {
                    if c2.is_ascii_alphabetic() || c2 == 'm' {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Results of a recorded run, serialized to results.json.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RunResults {
    pub run_id: String,
    pub passed: usize,
    pub failed: usize,
    pub paths: Vec<RecordedPath>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RecordedPath {
    pub name: String,
    pub passed: bool,
    pub steps: Vec<RecordedStep>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct RecordedStep {
    pub index: usize,
    pub transition_name: String,
    pub source: String,
    pub target: String,
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub cast_file: String,
}

/// Write results.json to the run output directory.
pub fn write_results(output_dir: &Utf8Path, results: &RunResults) -> std::io::Result<()> {
    std::fs::create_dir_all(output_dir)?;
    let json = serde_json::to_string_pretty(results)?;
    std::fs::write(output_dir.join("results.json"), json)?;
    Ok(())
}

/// Read results.json from a run directory.
pub fn read_results(results_path: &Utf8Path) -> std::io::Result<RunResults> {
    let content = std::fs::read_to_string(results_path)?;
    let results: RunResults = serde_json::from_str(&content)?;
    Ok(results)
}

/// Find a run directory under `<root>/<config_dir>/runs/`. Returns the
/// named run, or the latest run when the caller names none.
pub fn find_run_dir(
    root: &Utf8Path,
    config_dir: &str,
    run_id: Option<&str>,
) -> error::Result<Utf8PathBuf> {
    let runs_dir = root.join(config_dir).join("runs");
    if !runs_dir.exists() {
        return Err(error::Error::NoRecordedRuns);
    }

    if let Some(id) = run_id {
        let run_dir = runs_dir.join(id);
        if run_dir.exists() {
            return Ok(run_dir);
        }
        return Err(error::Error::NoRecordedRuns);
    }

    let mut entries: Vec<Utf8PathBuf> = std::fs::read_dir(&runs_dir)
        .map_err(error::Error::Io)?
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().ok()?.is_dir() {
                Utf8PathBuf::try_from(e.path()).ok()
            } else {
                None
            }
        })
        .collect();

    if entries.is_empty() {
        return Err(error::Error::NoRecordedRuns);
    }

    entries.sort();
    Ok(entries.pop().unwrap())
}

/// HTML-escape a string.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Generate an HTML report — rendered markdown with code blocks for output.
pub fn generate_html_report(run_dir: &Utf8Path) -> std::io::Result<String> {
    let results = read_results(&run_dir.join("results.json"))?;

    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<title>Missouri Test Report</title>\n");

    html.push_str("<style>\n");
    html.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; color: #24292f; padding: 2em; max-width: 960px; margin: 0 auto; line-height: 1.5; }\n");
    html.push_str(".pass { color: #1a7f37; } .fail { color: #cf222e; }\n");
    html.push_str("h1 { border-bottom: 1px solid #d0d7de; padding-bottom: 0.3em; }\n");
    html.push_str("pre { background: #f6f8fa; border: 1px solid #d0d7de; border-radius: 6px; padding: 1em; overflow-x: auto; font-size: 0.85em; line-height: 1.45; }\n");
    html.push_str(
        "code { font-family: 'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, monospace; }\n",
    );
    html.push_str(".cmd { color: #6e7781; }\n");
    html.push_str("</style>\n");

    html.push_str("</head>\n<body>\n");

    let status = if results.failed == 0 { "PASS" } else { "FAIL" };
    let status_class = if results.failed == 0 { "pass" } else { "fail" };
    html.push_str(&format!(
        "<h1>Missouri Test Report — <span class=\"{status_class}\">{status}</span></h1>\n"
    ));
    html.push_str(&format!(
        "<p>Run: <strong>{}</strong> | {} passed, {} failed</p>\n",
        html_escape(&results.run_id),
        results.passed,
        results.failed
    ));

    for path_result in &results.paths {
        let path_status = if path_result.passed { "PASS" } else { "FAIL" };
        let path_class = if path_result.passed { "pass" } else { "fail" };
        html.push_str(&format!(
            "<h2><span class=\"{path_class}\">{path_status}</span> {}</h2>\n",
            html_escape(&path_result.name)
        ));

        for step in &path_result.steps {
            let step_status = if step.passed { "PASS" } else { "FAIL" };
            let step_class = if step.passed { "pass" } else { "fail" };
            html.push_str(&format!(
                "<h3><span class=\"{step_class}\">{step_status}</span> {} → {} ({})</h3>\n",
                html_escape(&step.source),
                html_escape(&step.target),
                html_escape(&step.transition_name)
            ));

            let cast_path = run_dir.join(&step.cast_file);
            let output = extract_cast_output(&cast_path);
            html.push_str("<pre><code>");
            html.push_str(&format!(
                "<span class=\"cmd\">$ {}</span>\n",
                html_escape(&step.transition_name)
            ));
            if let Some(ref text) = output {
                html.push_str(&html_escape(text));
                html.push('\n');
            }
            html.push_str("</code></pre>\n");
        }
    }

    html.push_str("</body>\n</html>\n");
    Ok(html)
}

/// Generate a markdown report for a recorded run.
pub fn generate_md_report(run_dir: &Utf8Path) -> std::io::Result<String> {
    let results = read_results(&run_dir.join("results.json"))?;

    let mut md = String::new();
    let status = if results.failed == 0 { "PASS" } else { "FAIL" };
    md.push_str(&format!("# Missouri Test Report — {status}\n\n"));
    md.push_str(&format!(
        "Run: **{}** | {} passed, {} failed\n\n",
        results.run_id, results.passed, results.failed
    ));

    for path_result in &results.paths {
        let path_status = if path_result.passed { "PASS" } else { "FAIL" };
        md.push_str(&format!("## {path_status} {}\n\n", path_result.name));

        for step in &path_result.steps {
            let step_status = if step.passed { "PASS" } else { "FAIL" };
            md.push_str(&format!(
                "### {step_status} {} → {} ({})\n\n",
                step.source, step.target, step.transition_name
            ));

            let cast_path = run_dir.join(&step.cast_file);
            let output = extract_cast_output(&cast_path);
            md.push_str("```\n");
            md.push_str(&format!("$ {}\n", step.transition_name));
            if let Some(ref text) = output {
                md.push_str(text);
                md.push('\n');
            }
            md.push_str("```\n\n");
        }
    }

    Ok(md)
}

/// Print a terminal report for a recorded run.
pub fn print_terminal_report(run_dir: &Utf8Path) -> std::io::Result<()> {
    let results = read_results(&run_dir.join("results.json"))?;

    let status = if results.failed == 0 { "PASS" } else { "FAIL" };
    println!(
        "{status}: run {} — {} passed, {} failed",
        results.run_id, results.passed, results.failed
    );

    for path_result in &results.paths {
        let path_status = if path_result.passed { "PASS" } else { "FAIL" };
        println!("  {path_status} {}", path_result.name);

        for step in &path_result.steps {
            let icon = if step.passed { "✓" } else { "✗" };
            println!(
                "    {icon} {} → {} ({})",
                step.source, step.target, step.transition_name
            );
        }
    }

    Ok(())
}
