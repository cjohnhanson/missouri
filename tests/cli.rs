use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;

fn missouri() -> Command {
    assert_cmd::cargo_bin_cmd!("missouri")
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

/// Copy a fixture to a temp directory (for tests that write into the fixture).
fn copy_fixture_to_tmp(fixture_name: &str) -> tempfile::TempDir {
    let tmp = tmpdir();
    let fixture_path = fixture(fixture_name);
    let src = Path::new(&fixture_path);
    copy_dir_recursive(src, tmp.path());
    tmp
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

// --- -C flag (change directory) ---

#[test]
fn dash_c_changes_directory() {
    missouri()
        .arg("-C")
        .arg(fixture("01-trivial"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 01: Trivial (two states, one transition, no comparators) ---

#[test]
fn trivial_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn trivial_list_states() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .arg("--show")
        .arg("states")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn trivial_list_paths() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn trivial_validate() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("01-trivial"))
        .assert()
        .success();
}

// --- 02: Linear chain (A → B → C) ---

#[test]
fn linear_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("02-linear"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn linear_discovers_single_path() {
    // A → B → C is one path with two transitions
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("02-linear"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-c"));
}

// --- 03: Branching (root → left, root → right) ---

#[test]
fn branching_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("03-branching"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn branching_discovers_two_paths() {
    // root → left and root → right
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("03-branching"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("left"))
        .stdout(predicate::str::contains("right"));
}

// --- 04: Custom comparator (JSON semantic equality) ---

#[test]
fn custom_comparator_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("04-custom-comparator"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 05: Environment variables ---

#[test]
fn env_vars_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("05-env-vars"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 06: Ignore pattern ---

#[test]
fn ignore_pattern_run_passes() {
    // timestamp.txt differs but is ignored, so test should pass
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("06-ignore"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- 07: Cycle (A → B → A, no roots, should error or handle gracefully) ---

#[test]
fn cycle_no_roots_errors() {
    // Both states have inbound transitions, so there are no entry points.
    // missouri should report this as an error.
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

#[test]
fn cycle_validate_reports_no_roots() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("07-cycle"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("no entry points"));
}

// --- 12: Assertions (transition output + state assertions) ---

#[test]
fn assertions_full_mode_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("assert: check original content"))
        .stdout(predicate::str::contains(
            "assert: check transformed content",
        ))
        .stdout(predicate::str::contains("assert: bin script check"));
}

#[test]
fn assertions_check_only_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--check-only")
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("assert: check original content"))
        .stdout(predicate::str::contains(
            "assert: check transformed content",
        ));
}

#[test]
fn assertions_no_check_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--no-check")
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        // No assertions should appear
        .stdout(predicate::str::contains("assert").not());
}

#[test]
fn assertions_flags_conflict() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("--check-only")
        .arg("--no-check")
        .assert()
        .failure();
}

// --- 12: Project-level shared bin/ ---
// After deduplication, check-data lives in .missouri/bin/ at the root
// and both states find it via project bin on PATH.
#[test]
fn assertions_project_bin_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("12-assertions"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("assert: bin script check"));
}

// --- 13: Setup commands ---

#[test]
fn setup_runs_before_paths() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("13-setup"))
        .arg("-v")
        .assert()
        .success()
        .stdout(predicate::str::contains("setup"))
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn setup_failure_stops_execution() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("14-setup-fail"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("setup"));
}

// --- init command ---

#[test]
fn init_creates_project_structure() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    assert!(tmp.path().join(".missouri").is_dir());
    assert!(tmp.path().join(".missouri/missouri.yml").is_file());
    assert!(tmp.path().join(".missouri/bin").is_dir());
    assert!(tmp.path().join(".missouri/ignore").is_file());
}

#[test]
fn init_with_custom_config_dir() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri()
        .arg("--config-dir")
        .arg(".test-config")
        .arg("init")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".test-config").is_dir());
    assert!(tmp.path().join(".test-config/missouri.yml").is_file());
    assert!(tmp.path().join(".test-config/bin").is_dir());
}

#[test]
fn init_fails_if_already_initialized() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();

    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("init")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already initialized"));
}

// --- state add command ---

#[test]
fn state_add_creates_empty_state() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join("my-state").is_dir());
    assert!(tmp.path().join("my-state/.missouri/missouri.yml").is_file());
}

#[test]
fn state_add_fails_if_state_exists() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("my-state")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("already exists"));
}

#[test]
fn state_add_from_copies_state_and_creates_transition() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("before")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Add a data file to the source state
    fs::write(tmp.path().join("before/data.txt"), "hello\n").unwrap();

    missouri()
        .arg("state")
        .arg("add")
        .arg("after")
        .arg("--from")
        .arg("before")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Data file was copied
    assert_eq!(
        fs::read_to_string(tmp.path().join("after/data.txt")).unwrap(),
        "hello\n"
    );
    // Config was copied
    assert!(tmp.path().join("after/.missouri/missouri.yml").is_file());
    // Transition was appended to source
    let yml = fs::read_to_string(tmp.path().join("before/.missouri/missouri.yml")).unwrap();
    assert!(yml.contains("../after"));
    assert!(yml.contains("TODO"));
}

#[test]
fn state_add_from_nonexistent_fails() {
    let tmp = tmpdir();
    let dir = tmp.path().to_str().unwrap();
    missouri().arg("init").arg("-d").arg(dir).assert().success();

    missouri()
        .arg("state")
        .arg("add")
        .arg("after")
        .arg("--from")
        .arg("nonexistent")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("not found"));
}

// --- 15: Fail mid-path ---

#[test]
fn fail_mid_path_run_fails() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("15-fail-mid-path"))
        .assert()
        .failure()
        .stdout(predicate::str::contains("FAIL"))
        .stdout(predicate::str::contains("step one"))
        .stdout(predicate::str::contains("step two fails"));
}

// --- Recording ---

#[test]
fn record_produces_output_directory() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".missouri/runs/clitest").is_dir());
    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/results.json")
            .is_file()
    );
    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/path-0/step-0.cast")
            .is_file()
    );
}

#[test]
fn record_cast_files_per_step() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let runs_dir = tmp.path().join(".missouri/runs/clitest");
    // 03-branching has 2 paths, each with 1 step = 2 .cast files total
    assert!(runs_dir.join("path-0/step-0.cast").is_file());
    assert!(runs_dir.join("path-1/step-0.cast").is_file());
    // No step-1 in either path
    assert!(!runs_dir.join("path-0/step-1.cast").exists());
    assert!(!runs_dir.join("path-1/step-1.cast").exists());
}

#[test]
fn record_does_not_break_pass_fail() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"))
        .stdout(predicate::str::contains("2 passed"));
}

#[test]
fn record_with_failing_fixture() {
    let tmp = copy_fixture_to_tmp("15-fail-mid-path");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure();

    let runs_dir = tmp.path().join(".missouri/runs/clitest");
    assert!(runs_dir.join("results.json").is_file());
    // Step 0 ran and succeeded
    assert!(runs_dir.join("path-0/step-0.cast").is_file());
    // Step 1 ran and failed (but output was still captured)
    assert!(runs_dir.join("path-0/step-1.cast").is_file());
    // Step 2 never ran
    assert!(!runs_dir.join("path-0/step-2.cast").exists());
}

#[test]
fn record_run_id_flag() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("my-custom-id")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(tmp.path().join(".missouri/runs/my-custom-id").is_dir());
}

#[test]
fn record_default_run_id_is_timestamp() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let runs_dir = tmp.path().join(".missouri/runs");
    assert!(runs_dir.is_dir());
    let entries: Vec<_> = fs::read_dir(&runs_dir).unwrap().collect();
    assert_eq!(entries.len(), 1);
    let dirname = entries[0].as_ref().unwrap().file_name();
    let dirname = dirname.to_str().unwrap();
    // Should look like 2026-02-22T17-30-00
    assert!(
        dirname.len() >= 19,
        "expected timestamp-like dirname, got: {dirname}"
    );
    assert!(
        dirname.contains('T'),
        "expected timestamp-like dirname, got: {dirname}"
    );
}

#[test]
fn record_conflicts_with_check_only() {
    missouri()
        .arg("run")
        .arg("--record")
        .arg("--check-only")
        .arg("-d")
        .arg(fixture("03-branching"))
        .assert()
        .failure();
}

#[test]
fn record_works_with_no_check() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--no-check")
        .arg("--run-id")
        .arg("clitest")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    assert!(
        tmp.path()
            .join(".missouri/runs/clitest/path-0/step-0.cast")
            .is_file()
    );
}

// --- Reporting ---

#[test]
fn report_terminal_default() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    // Setup: record a run
    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Report
    missouri()
        .arg("report")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"))
        .stdout(predicate::str::contains("root"));
}

#[test]
fn report_html_generates_file() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("html")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    // Find the generated HTML file
    let report_path = tmp.path().join(".missouri/runs/r1/report.html");
    assert!(report_path.is_file(), "HTML report should be generated");
    let html = fs::read_to_string(&report_path).unwrap();
    assert!(html.contains("<!DOCTYPE html>"));
    assert!(html.contains("<pre><code>"));
    assert!(html.contains("PASS") || html.contains("FAIL"));
}

#[test]
fn report_html_is_self_contained() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("html")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let html = fs::read_to_string(tmp.path().join(".missouri/runs/r1/report.html")).unwrap();
    assert!(html.contains("<pre><code>"));
    // No external resources
    assert!(!html.contains("<script src="));
    assert!(!html.contains("<link href="));
}

#[test]
fn report_md_generates_file() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("r1")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--format")
        .arg("md")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    let report_path = tmp.path().join(".missouri/runs/r1/report.md");
    assert!(report_path.is_file(), "Markdown report should be generated");
    let md = fs::read_to_string(&report_path).unwrap();
    assert!(md.contains("```"));
    assert!(md.contains("root"));
}

#[test]
fn report_specific_run() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("run-a")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("run")
        .arg("--record")
        .arg("--run-id")
        .arg("run-b")
        .arg("-d")
        .arg(dir)
        .assert()
        .success();

    missouri()
        .arg("report")
        .arg("--run")
        .arg("run-a")
        .arg("-d")
        .arg(dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("run-a"));
}

#[test]
fn report_no_runs_errors() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("report")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recorded runs"));
}

// --- Serving ---

#[test]
fn serve_no_runs_errors() {
    let tmp = copy_fixture_to_tmp("03-branching");
    let dir = tmp.path().to_str().unwrap();

    missouri()
        .arg("serve")
        .arg("-d")
        .arg(dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recorded runs"));
}

// --- 18: Root-level missouri.yml with test_dir ---

#[test]
fn test_dir_run_passes() {
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn test_dir_list_states() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .arg("--show")
        .arg("states")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn test_dir_list_paths() {
    missouri()
        .arg("list")
        .arg("-d")
        .arg(fixture("18-test-dir"))
        .arg("--show")
        .arg("paths")
        .assert()
        .success()
        .stdout(predicate::str::contains("state-a"))
        .stdout(predicate::str::contains("state-b"));
}

#[test]
fn test_dir_dash_c_works() {
    // -C should work with root-level missouri.yml
    missouri()
        .arg("-C")
        .arg(fixture("18-test-dir"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("PASS"));
}

// --- workspace (members) ---

#[test]
fn workspace_run_passes_all_members() {
    // Workspace mode should print member headers and per-member summaries
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .assert()
        .success()
        // Member section headers
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"))
        // Per-member pass lines
        .stdout(predicate::str::contains("PASS"));
}

#[test]
fn workspace_run_has_per_member_summaries() {
    // Each member should produce its own summary line with pass/fail counts
    let output = missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Two separate summary lines (one per member)
    let summary_count = stdout.matches("passed").count();
    assert!(
        summary_count >= 2,
        "expected at least 2 summary lines, got {summary_count}: {stdout}"
    );
}

#[test]
fn workspace_run_fails_if_any_member_fails() {
    // When one member has failing tests, the overall result should be failure
    missouri()
        .arg("run")
        .arg("-d")
        .arg(fixture("20-workspace-fail"))
        .assert()
        .failure()
        // The failing member should appear in output
        .stdout(predicate::str::contains("bad-sub"))
        .stdout(predicate::str::contains("FAIL"));
}

#[test]
fn workspace_dash_c_works() {
    // -C should work with workspace missouri.yml
    missouri()
        .arg("-C")
        .arg(fixture("19-workspace"))
        .arg("run")
        .assert()
        .success()
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"));
}

#[test]
fn workspace_validate_checks_all_members() {
    missouri()
        .arg("validate")
        .arg("-d")
        .arg(fixture("19-workspace"))
        .assert()
        .success()
        // Should report valid state for both members
        .stdout(predicate::str::contains("sub-a"))
        .stdout(predicate::str::contains("sub-b"));
}
