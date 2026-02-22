use assert_cmd::Command;
use predicates::prelude::*;

fn missouri() -> Command {
    Command::cargo_bin("missouri").unwrap()
}

fn fixture(name: &str) -> String {
    format!("tests/fixtures/{name}")
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
