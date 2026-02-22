use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;

fn missouri() -> Command {
    Command::cargo_bin("missouri").unwrap()
}

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn tmpdir() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
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
